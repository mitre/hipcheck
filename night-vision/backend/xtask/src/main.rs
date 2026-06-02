pub mod command;
pub mod workspace;

use anyhow::Result;
use clap::{Arg, Command, value_parser};
use std::{path::PathBuf, process::ExitCode};

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
        .subcommand(Command::new("ci").about("Run CI checks for the workspace"))
        .subcommand(Command::new("lint").about("Lint crates in the workspace"))
        .subcommand(
            Command::new("unit-graph")
                .about("Generate a unit graph visualizing Cargo builds (experimental)")
                .long_about("\
Generates a visualization of Cargo's \"unit graph\" (the individual codegen steps Cargo will take to complete a build)
based on the output of the `--unit-graph` flag from Cargo.

The `--unit-graph` flag is unstable, but this project uses a pinned stable toolchain rather than Cargo nightly.
Use `RUSTC_BOOTSTRAP=1` with `-Z unstable-options` to enable the unstable Cargo flags for this invocation.
`RUSTC_BOOTSTRAP` is itself a permanently-unstable escape hatch, so use it only for this kind of narrow tooling command.
Then you can pipe the output to `cargo xtask unit-graph`, using `-` to indicate you're reading from `stdin`.

Invocations look like:

$ RUSTC_BOOTSTRAP=1 cargo -Z unstable-options <BUILD_CMD> --unit-graph | cargo xtask unit-graph - \
                    ")
                .arg(
                    Arg::new("input")
                        .help("input file name or `-` to read from stdin")
                        .default_value("-")
                        .value_parser(value_parser!(PathBuf))
                        .required(true),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("add", cmd)) => command::add(cmd)?,
        Some(("ci", cmd)) => command::ci(cmd)?,
        Some(("lint", cmd)) => command::lint(cmd)?,
        Some(("unit-graph", cmd)) => command::unit_graph(cmd)?,
        Some(_) => unimplemented!("unknown command"),
        None => {}
    }

    Ok(())
}
