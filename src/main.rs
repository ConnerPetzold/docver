use std::path::PathBuf;

use clap::{Args, Parser};

use crate::commands::Command;

mod commands;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    git_args: GitArgs,
}

#[derive(Debug, Args)]
struct GitArgs {
    #[arg(short, long, default_value = "origin", global = true)]
    remote: String,

    #[arg(short, long, default_value = "gh-pages", global = true)]
    branch: String,

    #[arg(short, long, global = true)]
    message: Option<String>,

    #[arg(short, long, global = true)]
    push: bool,

    #[arg(long, global = true)]
    allow_empty: bool,

    #[arg(long, global = true)]
    deploy_prefix: Option<PathBuf>,

    #[arg(long, global = true)]
    ignore_remote_status: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.command.execute(cli.git_args)?;

    Ok(())
}
