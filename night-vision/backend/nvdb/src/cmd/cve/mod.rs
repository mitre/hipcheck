use anyhow::Result;
use nv_common::config::Config;

pub mod sync;

pub fn command() -> clap::Command {
    clap::Command::new("cve")
        .about("Manage CVE List data")
        .arg_required_else_help(true)
        .subcommand(sync::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(sync_matches) = matches.subcommand_matches("sync") {
        return sync::run(config, sync_matches);
    }

    Ok(())
}
