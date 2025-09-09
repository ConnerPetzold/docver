use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

const DEFAULT_AUTHOR_NAME: &str = concat!(env!("CARGO_PKG_NAME"), "[bot]");
const DEFAULT_AUTHOR_EMAIL: &str = concat!(env!("CARGO_PKG_NAME"), "[bot]@users.noreply.github.io");

#[derive(Debug, Clone)]
pub struct Commit {
    repo_dir: PathBuf,
    refname: String,
    author: Option<(String, String, String)>,
    committer: Option<(String, String, String)>,
    message: String,
    from: Option<String>,
    delete_all: bool,
    deletes: BTreeMap<String, ()>,
    files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Clone)]
enum FileEntry {
    Inline { mode: u32, data: Vec<u8> },
}

impl Commit {
    pub fn new(repo_dir: impl Into<PathBuf>, refname: impl Into<String>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            refname: refname.into(),
            author: None,
            committer: None,
            message: String::new(),
            from: None,
            delete_all: false,
            deletes: BTreeMap::new(),
            files: BTreeMap::new(),
        }
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn now_when() -> String {
        let secs: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        format!("{} +0000", secs)
    }

    pub fn parent(mut self, commit: impl Into<String>) -> Self {
        self.from = Some(commit.into());
        self
    }

    pub fn delete_path(mut self, path: impl AsRef<str>) -> Self {
        self.deletes.insert(path.as_ref().to_string(), ());
        self
    }

    pub fn add_bytes(
        mut self,
        path: impl AsRef<str>,
        mode: u32,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.files.insert(
            path.as_ref().to_string(),
            FileEntry::Inline {
                mode,
                data: bytes.into(),
            },
        );
        self
    }

    pub fn add_file(self, dest_path: impl AsRef<str>, src: impl AsRef<Path>) -> Result<Self> {
        let data = fs::read(src.as_ref()).with_context(|| {
            format!(
                "failed to read file for fast-import: {}",
                src.as_ref().display()
            )
        })?;
        Ok(self.add_bytes(dest_path, 0o100644, data))
    }

    fn resolve_author(&self) -> (String, String, String) {
        if let Some((n, e, t)) = &self.author {
            return (n.clone(), e.clone(), t.clone());
        }
        let name = get_env_value("AUTHOR", "NAME")
            .or_else(|| get_env_value("COMMITTER", "NAME"))
            .unwrap_or_else(|| DEFAULT_AUTHOR_NAME.to_string());
        let email = get_env_value("AUTHOR", "EMAIL")
            .or_else(|| get_env_value("COMMITTER", "EMAIL"))
            .unwrap_or_else(|| DEFAULT_AUTHOR_EMAIL.to_string());
        let when = get_env_value("AUTHOR", "DATE").unwrap_or_else(Self::now_when);
        (name, email, when)
    }

    fn resolve_committer(
        &self,
        default_name: &str,
        default_email: &str,
        default_when: &str,
    ) -> (String, String, String) {
        if let Some((n, e, t)) = &self.committer {
            return (n.clone(), e.clone(), t.clone());
        }
        let name = get_env_value("COMMITTER", "NAME").unwrap_or_else(|| default_name.to_string());
        let email =
            get_env_value("COMMITTER", "EMAIL").unwrap_or_else(|| default_email.to_string());
        let when = get_env_value("COMMITTER", "DATE").unwrap_or_else(|| default_when.to_string());
        (name, email, when)
    }

    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        writeln!(w, "commit {}", self.refname)?;

        let (an, ae, at_when) = self.resolve_author();
        let (cn, ce, ct_when) = self.resolve_committer(&an, &ae, &at_when);

        writeln!(w, "author {}<{}> {}", name_field(&an), ae, at_when)?;
        writeln!(w, "committer {}<{}> {}", name_field(&cn), ce, ct_when)?;

        writeln!(w, "data {}", self.message.len())?;
        writeln!(w, "{}", self.message)?;

        if let Some(from) = &self.from {
            writeln!(w, "from {}", from)?;
        }
        if self.delete_all {
            writeln!(w, "deleteall")?;
        }

        for (path, _) in &self.deletes {
            writeln!(w, "D {}", path)?;
        }

        for (path, entry) in &self.files {
            match entry {
                FileEntry::Inline { mode, data } => {
                    writeln!(w, "M {:06o} inline {}", mode, path)?;
                    writeln!(w, "data {}", data.len())?;
                    w.write_all(data)?;
                    writeln!(w)?;
                }
            }
        }

        writeln!(w, "done")?;
        Ok(())
    }

    pub fn run(&self) -> Result<()> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.repo_dir)
            .arg("fast-import")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn git fast-import")?;

        {
            let stdin = child.stdin.take().expect("stdin should be piped");
            let mut bufw = io::BufWriter::new(stdin);
            self.write_to(&mut bufw)?;
        }

        let output = child
            .wait_with_output()
            .context("failed to wait on git fast-import")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_trimmed = stderr.trim();

            // Provide a more readable hint for common non-fast-forward failures
            if stderr_trimmed.contains("Not updating")
                && (stderr_trimmed.contains("does not contain")
                    || stderr_trimmed.contains("non-fast-forward"))
            {
                anyhow::bail!(
                    "git fast-import refused to update {} (non-fast-forward). The new commit must descend from the current branch tip. Hint: base the import on the tip (set a parent) or recreate/reset the branch.\nFull error: {}",
                    self.refname,
                    stderr_trimmed
                );
            }

            anyhow::bail!("git fast-import failed: {}", stderr_trimmed);
        }
        Ok(())
    }
}

fn name_field(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("{} ", name)
    }
}

fn sanitize_identity_part(s: &str) -> String {
    s.replace(['<', '>', '\n'], "")
}

fn get_env_value(scope: &str, field: &str) -> Option<String> {
    let key = format!("GIT_{}_{}", scope, field);
    std::env::var(key).ok().map(|s| sanitize_identity_part(&s))
}
