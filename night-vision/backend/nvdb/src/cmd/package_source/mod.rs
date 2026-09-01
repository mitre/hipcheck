use anyhow::Result;
use nv_common::config::Config;

pub mod import;
pub mod kevs;
pub mod resolve;
pub mod show;
pub mod versions;

pub fn command() -> clap::Command {
    clap::Command::new("package-source")
        .about("Manage package sources and their resolved versions")
        .arg_required_else_help(true)
        .subcommand(import::command())
        .subcommand(kevs::command())
        .subcommand(resolve::command())
        .subcommand(show::command())
        .subcommand(versions::command())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(import_matches) = matches.subcommand_matches("import") {
        return import::run(config, import_matches);
    }

    if let Some(kevs_matches) = matches.subcommand_matches("kevs") {
        return kevs::run(config, kevs_matches);
    }

    if let Some(resolve_matches) = matches.subcommand_matches("resolve") {
        return resolve::run(config, resolve_matches);
    }

    if let Some(show_matches) = matches.subcommand_matches("show") {
        return show::run(config, show_matches);
    }

    if let Some(versions_matches) = matches.subcommand_matches("versions") {
        return versions::run(config, versions_matches);
    }

    Ok(())
}
