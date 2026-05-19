pub mod command;

use anyhow::Result;
use clap::{Arg, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let matches = Command::new("xtask")
        .about("Task runner for the Night Vision backend")
        .arg_required_else_help(true)
        .subcommand(
            Command::new("add")
                .about("`cargo add` wrapper that fixes up workspace-hack and autoinherits")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("all")
                        .help("pass through flags to `cargo add`")
                        .allow_hyphen_values(true)
                        .num_args(..)
                        .trailing_var_arg(true),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("add", cmd)) => command::add(cmd)?,
        Some(_) => unimplemented!("unknown command"),
        None => {}
    }

    Ok(())
}
