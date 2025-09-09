use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use git_cmd::git_in_dir;
use walkdir::WalkDir;

use crate::{
    GitArgs,
    git::Commit,
    versions::{VERSIONS_FILE, Versions},
};

#[derive(Debug, Args)]
/// Deploy a built static site version to the target branch
pub struct DeployArgs {
    /// Path to the directory containing the built site to deploy
    path: PathBuf,

    /// Version identifier for this deployment (e.g. "v1.2.3" or "1.0")
    version: String,

    /// Additional aliases that should point to this version (e.g. "latest")
    aliases: Vec<String>,

    /// Optional human-readable title for this version
    #[arg(short, long)]
    title: Option<String>,
}

impl DeployArgs {
    pub fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        let commit_sha = git_in_dir(".".into(), &["show", "-s", "--format=%h"])?;

        let message = git_args.message.clone().unwrap_or(format!(
            "Deployed {} to {}{} with {} {}",
            commit_sha,
            self.version,
            git_args
                .deploy_prefix
                .as_ref()
                .map(|p| format!("in {}", p.display()))
                .unwrap_or_default(),
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ));

        git_in_dir(
            ".".into(),
            &["fetch", git_args.remote.as_str(), git_args.branch.as_str()],
        )?;

        let remote_rev = git_args.remote_rev();
        let mut versions: Versions = Versions::from_git(&remote_rev);

        versions.add(
            self.version.clone(),
            self.title.clone(),
            self.aliases.clone().into_iter().collect(),
        );

        let versions_json = serde_json::to_string_pretty(&versions)
            .context(format!("Failed to serialize {}", VERSIONS_FILE))?;

        let deploy_prefix = git_args.deploy_prefix.clone().unwrap_or_default();

        let main_version_path = deploy_prefix.join(self.version.clone());

        let parent_head = git_in_dir(".".into(), &["rev-parse", remote_rev.as_str()])
            .or_else(|_| git_in_dir(".".into(), &["rev-parse", git_args.branch.as_str()]))
            .ok();

        let mut commit =
            Commit::new(".", format!("refs/heads/{}", git_args.branch)).message(message.clone());

        if let Some(parent) = parent_head {
            commit = commit.parent(parent.trim().to_string());
        }

        commit = commit.add_bytes(VERSIONS_FILE, 0o100644, versions_json.into_bytes());

        // TODO: make the default alias configurable
        let rewrites = versions.netlify_rewrites("latest".into());
        commit = commit.add_bytes("_redirects", 0o100644, rewrites.into_bytes());

        if std::path::Path::new(".gitignore").exists() {
            commit = commit.add_file(".gitignore", ".gitignore")?;
        }

        if git_in_dir(
            ".".into(),
            &["show", format!("{}:{}", remote_rev, ".nojekyll").as_str()],
        )
        .or_else(|_| {
            git_in_dir(
                ".".into(),
                &[
                    "show",
                    format!("{}:{}", git_args.branch, ".nojekyll").as_str(),
                ],
            )
        })
        .is_err()
        {
            commit = commit.add_bytes(".nojekyll", 0o100644, Vec::<u8>::new());
        }

        commit = commit.delete_path(main_version_path.to_string_lossy());

        for entry in WalkDir::new(&self.path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let rel = path.strip_prefix(&self.path).unwrap();
            let dest = main_version_path.join(rel);
            let dest_str = dest.to_string_lossy().to_string();
            commit = commit.add_file(dest_str, path)?;
        }

        commit.run()?;

        // Print a concise success message for local import
        println!("Deployed to {} (local).", git_args.branch);

        if git_args.push {
            git_in_dir(
                ".".into(),
                &["push", git_args.remote.as_str(), git_args.branch.as_str()],
            )?;

            // Print a concise success message for push
            println!(
                "Pushed {} to {}:{}",
                git_args.branch, git_args.remote, git_args.branch
            );
        }

        Ok(())
    }
}
