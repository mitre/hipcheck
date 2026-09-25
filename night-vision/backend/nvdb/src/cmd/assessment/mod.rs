use anyhow::Result;
use nv_common::config::Config;

pub mod analyze;
pub mod evidence;
pub mod runs;
pub mod show;

pub fn command() -> clap::Command {
    clap::Command::new("assessment")
        .about("Run and inspect package-version assessments")
        .arg_required_else_help(true)
        .subcommand(analyze::command())
        .subcommand(evidence::command())
        .subcommand(runs::command())
        .subcommand(show::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(analyze_matches) = matches.subcommand_matches("analyze") {
        return analyze::run(config, analyze_matches);
    }

    if let Some(evidence_matches) = matches.subcommand_matches("evidence") {
        return evidence::run(config, evidence_matches);
    }

    if let Some(runs_matches) = matches.subcommand_matches("runs") {
        return runs::run(config, runs_matches);
    }

    if let Some(show_matches) = matches.subcommand_matches("show") {
        return show::run(config, show_matches);
    }

    Ok(())
}
