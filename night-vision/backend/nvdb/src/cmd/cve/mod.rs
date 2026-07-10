use anyhow::Result;
use nv_common::config::Config;

pub mod config;
pub mod doctor;
pub mod list;
pub mod record;
pub mod recover;
pub mod reset;
pub mod run;
pub mod runs;
pub mod stats;
pub mod status;
pub mod sync;

pub fn command() -> clap::Command {
    clap::Command::new("cve")
        .about("Manage CVE List data")
        .arg_required_else_help(true)
        .subcommand(config::command())
        .subcommand(doctor::command())
        .subcommand(list::command())
        .subcommand(record::command())
        .subcommand(recover::command())
        .subcommand(reset::command())
        .subcommand(run::command())
        .subcommand(runs::command())
        .subcommand(stats::command())
        .subcommand(status::command())
        .subcommand(sync::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(config_matches) = matches.subcommand_matches("config") {
        return self::config::run(config, config_matches);
    }

    if let Some(doctor_matches) = matches.subcommand_matches("doctor") {
        return doctor::run(config, doctor_matches);
    }

    if let Some(list_matches) = matches.subcommand_matches("list") {
        return list::run(config, list_matches);
    }

    if let Some(record_matches) = matches.subcommand_matches("record") {
        return record::run(config, record_matches);
    }

    if let Some(recover_matches) = matches.subcommand_matches("recover") {
        return recover::run(config, recover_matches);
    }

    if let Some(reset_matches) = matches.subcommand_matches("reset") {
        return reset::run(config, reset_matches);
    }

    if let Some(run_matches) = matches.subcommand_matches("run") {
        return run::run(config, run_matches);
    }

    if let Some(runs_matches) = matches.subcommand_matches("runs") {
        return runs::run(config, runs_matches);
    }

    if let Some(stats_matches) = matches.subcommand_matches("stats") {
        return stats::run(config, stats_matches);
    }

    if let Some(status_matches) = matches.subcommand_matches("status") {
        return status::run(config, status_matches);
    }

    if let Some(sync_matches) = matches.subcommand_matches("sync") {
        return sync::run(config, sync_matches);
    }

    Ok(())
}
