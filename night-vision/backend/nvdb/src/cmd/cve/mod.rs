use anyhow::Result;
use nv_common::config::Config;

pub mod record;
pub mod run;
pub mod runs;
pub mod status;
pub mod sync;

pub fn command() -> clap::Command {
    clap::Command::new("cve")
        .about("Manage CVE List data")
        .arg_required_else_help(true)
        .subcommand(record::command())
        .subcommand(run::command())
        .subcommand(runs::command())
        .subcommand(status::command())
        .subcommand(sync::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(record_matches) = matches.subcommand_matches("record") {
        return record::run(config, record_matches);
    }

    if let Some(run_matches) = matches.subcommand_matches("run") {
        return run::run(config, run_matches);
    }

    if let Some(runs_matches) = matches.subcommand_matches("runs") {
        return runs::run(config, runs_matches);
    }

    if let Some(status_matches) = matches.subcommand_matches("status") {
        return status::run(config, status_matches);
    }

    if let Some(sync_matches) = matches.subcommand_matches("sync") {
        return sync::run(config, sync_matches);
    }

    Ok(())
}
