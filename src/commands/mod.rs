use clap::Subcommand;

use crate::GitArgs;

mod deploy;

#[derive(Subcommand)]
pub enum Command {
    Deploy(deploy::DeployArgs),
}

impl Command {
    pub fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        match self {
            Command::Deploy(args) => args.execute(git_args)?,
        }

        Ok(())
    }
}
