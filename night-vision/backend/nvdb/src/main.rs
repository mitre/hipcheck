use anyhow::Result;
use nv_common::config::DEFAULT_CONFIG_FILE;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let matches = clap::Command::new("nvdb")
        .about("Night Vision debugger")
        .arg_required_else_help(true)
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .default_value(DEFAULT_CONFIG_FILE)
                .global(true)
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
                .arg_required_else_help(true),
        )
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("api") {
        todo!("api subcommand not yet implemented")
    }

    if let Some(_matches) = matches.subcommand_matches("db") {
        todo!("db subcommand not yet implemented")
    }

    Ok(())
}
