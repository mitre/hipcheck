use anyhow::Result;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let _matches = clap::Command::new("xtask")
        .about("Task runner for the Night Vision backend")
        .arg_required_else_help(true)
        .get_matches();

    Ok(())
}
