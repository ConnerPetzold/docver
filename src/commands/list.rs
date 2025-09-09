use crate::{GitArgs, versions::Versions};
use clap::Args;
use colored::Colorize;
use git_cmd::git_in_dir;

#[derive(Debug, Args)]
/// List all versions of the site
pub struct ListArgs {
    /// Version or alias identifiers to list
    identifiers: Vec<String>,

    /// Output in JSON format
    #[arg(short, long, default_value = "false")]
    json: bool,
}

impl ListArgs {
    pub fn execute(&self, git_args: GitArgs) -> anyhow::Result<()> {
        git_in_dir(
            ".".into(),
            &["fetch", git_args.remote.as_str(), git_args.branch.as_str()],
        )?;

        let versions = Versions::from_git(&git_args.remote_rev());

        if self.json {
            println!("{}", serde_json::to_string_pretty(&versions)?);
        } else {
            for (version, aliases) in &versions {
                print!("{}", version.tag.green());
                if let Some(title) = &version.title {
                    print!(" ({})", title.blue());
                }
                if !aliases.is_empty() {
                    print!(" [{}]", aliases.join(", ").yellow());
                }
                println!();
            }
        }

        Ok(())
    }
}
