use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use walkdir::WalkDir;

use crate::{GitArgs, git::Commit, versions::Versions};

const VERSIONS_FILE: &str = "versions.json";

#[derive(Debug, Args)]
pub struct DeployArgs {
    path: PathBuf,

    version: String,

    aliases: Vec<String>,

    #[arg(short, long)]
    title: Option<String>,

    #[arg(short, long, default_value = "false")]
    update_aliases: bool,
}

impl DeployArgs {
    pub fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        let repo = git_cmd::Repo::new(".")?;
        let commit_sha = repo.git(&["rev-parse", "--short", "HEAD"])?;

        let message = git_args.message.clone().unwrap_or(format!(
            "Deployed {} to {}{} with docver {}",
            commit_sha,
            self.version,
            git_args
                .deploy_prefix
                .as_ref()
                .map(|p| format!("in {}", p.display()))
                .unwrap_or_default(),
            env!("CARGO_PKG_VERSION")
        ));

        let mut versions: Versions = repo
            .git(&[
                "show",
                format!("{}:{}", git_args.branch, VERSIONS_FILE).as_str(),
            ])
            .and_then(|s| {
                serde_json::from_str(&s).context(format!("Failed to parse {}", VERSIONS_FILE))
            })
            .unwrap_or_default();

        versions.add(
            self.version.clone(),
            self.title.clone(),
            self.aliases.clone().into_iter().collect(),
        );

        let versions_json = serde_json::to_string_pretty(&versions)
            .context(format!("Failed to serialize {}", VERSIONS_FILE))?;

        let deploy_prefix = git_args.deploy_prefix.clone().unwrap_or_default();

        let main_version_path = deploy_prefix.join(self.version.clone());

        let alias_paths = self
            .aliases
            .iter()
            .map(|alias| deploy_prefix.join(alias))
            .collect::<Vec<_>>();

        // Build a fast-import commit that adds all files under `self.path`
        // into `main_version_path` and each of the `alias_paths`.
        let mut commit = Commit::new(
            repo.directory().as_str(),
            format!("refs/heads/{}", git_args.branch),
        )
        .message(message.clone())
        .delete_all();

        // We start from an empty tree so the commit only contains what we add

        // Add versions file at repository root
        commit = commit.add_bytes(VERSIONS_FILE, 0o100644, versions_json.into_bytes());
        // Copy root .gitignore if present
        if std::path::Path::new(".gitignore").exists() {
            commit = commit.add_file(".gitignore", ".gitignore")?;
        }

        // Walk source directory and add files to the commit under each destination root
        for entry in WalkDir::new(&self.path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let rel = path.strip_prefix(&self.path).unwrap();

            // main version
            let dest = main_version_path.join(rel);
            let dest_str = dest.to_string_lossy().to_string();
            dbg!(&dest_str);
            commit = commit.add_file(dest_str, path)?;
            // aliases
            for alias_root in &alias_paths {
                let dest = alias_root.join(rel);
                let dest_str = dest.to_string_lossy().to_string();
                dbg!(&dest_str);
                commit = commit.add_file(dest_str, path)?;
            }
        }

        // Execute the fast-import
        commit.run()?;

        Ok(())
    }
}
