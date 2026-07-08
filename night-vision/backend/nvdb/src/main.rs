use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Args as _, FromArgMatches as _};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use nv_common::config::{Config, DEFAULT_CONFIG_FILE};
use nv_common::log::logger;
use std::process::ExitCode;

mod cmd;
mod destructive;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let matches = command().get_matches();
    let verbosity_filter = Verbosity::<InfoLevel>::from_arg_matches(&matches)
        .expect("verbose and quiet args are always present and default to 0")
        .filter();
    let _log = logger(verbosity_filter);

    if let Some(_matches) = matches.subcommand_matches("api") {
        todo!("api subcommand not yet implemented")
    }

    let config_file = matches
        .get_one::<Utf8PathBuf>("config")
        .expect("config has a default value");
    let config = Config::parse(config_file)?;

    if let Some(db_matches) = matches.subcommand_matches("db") {
        if let Some(_matches) = db_matches.subcommand_matches("schema") {
            return cmd::db::schema::run(&config);
        }

        if let Some(entity_matches) = db_matches.subcommand_matches("entity") {
            return cmd::db::entity::run(&config, entity_matches);
        }

        if let Some(migrate_matches) = db_matches.subcommand_matches("migrate") {
            return cmd::db::migrate::run(&config, migrate_matches);
        }
    }

    if let Some(cve_matches) = matches.subcommand_matches("cve") {
        return cmd::cve::run(&config, cve_matches);
    }

    Ok(())
}

fn command() -> clap::Command {
    let command = clap::Command::new("nvdb")
        .about("Night Vision debugger")
        .arg_required_else_help(true)
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .default_value(DEFAULT_CONFIG_FILE)
                .global(true)
                .value_parser(clap::value_parser!(Utf8PathBuf))
                .help("Path to the configuration file"),
        )
        .subcommand(
            clap::Command::new("api")
                .about("Interact with the REST API")
                .arg_required_else_help(true),
        )
        .subcommand(
            clap::Command::new("db")
                .about("Manage the database")
                .arg_required_else_help(true)
                .subcommand(cmd::db::entity::command())
                .subcommand(cmd::db::schema::command())
                .subcommand(cmd::db::migrate::command()),
        )
        .subcommand(cmd::cve::command());

    Verbosity::<InfoLevel>::augment_args(command)
}
