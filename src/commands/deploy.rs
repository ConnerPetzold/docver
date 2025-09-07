use clap::Args;

use crate::GitArgs;

#[derive(Debug, Args)]
pub struct DeployArgs {
    version: String,

    aliases: Vec<String>,

    #[arg(short, long)]
    title: Option<String>,

    #[arg(short, long, default_value = "false")]
    update_aliases: bool,
}

impl DeployArgs {
    pub(crate) fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        let repo = git_cmd::Repo::new(".")?;

        let message = git_args.message.unwrap_or(format!(
            "Deployed {} to {}{} with docver {}",
            repo.current_commit_hash()?,
            self.version,
            git_args
                .deploy_prefix
                .map(|p| format!("{}", p.display()))
                .unwrap_or_default(),
            env!("CARGO_PKG_VERSION")
        ));

        println!("{}", message);

        Ok(())
    }
}

// 'Deployed {rev} to {doc_version}{deploy_prefix} with MkDocs ' +
//             '{mkdocs_version} and mike {mike_version}'
