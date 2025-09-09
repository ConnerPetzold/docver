use clap::Subcommand;

use crate::GitArgs;

mod deploy;
mod list;

#[derive(Subcommand)]
pub enum Command {
    Deploy(deploy::DeployArgs),
    List(list::ListArgs),
}

impl Command {
    pub fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        match self {
            Command::Deploy(args) => args.execute(git_args)?,
            Command::List(args) => args.execute(git_args)?,
        }

        Ok(())
    }
}
