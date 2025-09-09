use std::path::PathBuf;

use clap::{Args, Parser};

use crate::commands::Command;

mod commands;
mod git;
pub mod versions;

#[derive(Parser)]
#[command(version, about, long_about = None)]
/// Versite: versioned static site deployments to a Git branch
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Global git options applied to all subcommands
    #[command(flatten)]
    git_args: GitArgs,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Global Options")]
struct GitArgs {
    /// Git remote to push to (e.g. "origin")
    #[arg(short, long, default_value = "origin", global = true)]
    remote: String,

    /// Git branch to publish to (e.g. "gh-pages")
    #[arg(short, long, default_value = "gh-pages", global = true)]
    branch: String,

    /// Commit message to use for the deployment (defaults to an auto-generated message)
    #[arg(short, long, global = true)]
    message: Option<String>,

    /// Push the commit after creating it
    #[arg(short, long, global = true)]
    push: bool,

    /// Optional prefix directory under which to place deployed files
    #[arg(long, global = true)]
    deploy_prefix: Option<PathBuf>,
}

impl GitArgs {
    pub fn remote_rev(&self) -> String {
        format!("{}/{}", self.remote, self.branch)
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.command.execute(cli.git_args)?;

    Ok(())
}
