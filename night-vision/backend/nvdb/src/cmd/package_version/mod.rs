use anyhow::Result;
use nv_common::config::Config;

pub mod candidates;
pub mod kev;

pub fn command() -> clap::Command {
    clap::Command::new("package-version")
        .about("Inspect resolved package versions")
        .arg_required_else_help(true)
        .subcommand(candidates::command())
        .subcommand(kev::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(candidate_matches) = matches.subcommand_matches("candidates") {
        return candidates::run(config, candidate_matches);
    }
    if let Some(kev_matches) = matches.subcommand_matches("kev") {
        return kev::run(config, kev_matches);
    }

    Ok(())
}
