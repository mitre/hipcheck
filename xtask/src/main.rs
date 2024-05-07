// SPDX-License-Identifier: Apache-2.0

mod exit;
mod task;
mod workspace;

use crate::exit::EXIT_FAILURE;
use crate::exit::EXIT_SUCCESS;
use crate::task::doc::OpenDoc;
use anyhow::Error;
use anyhow::Result;
use clap::crate_version;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use std::process::exit;

fn main() {
	let matches = Command::new("xtask")
		.about("Hipcheck development task runner.")
		.version(crate_version!())
		.arg(
			Arg::new("help")
				.short('h')
				.long("help")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("version")
				.short('V')
				.long("version")
				.action(ArgAction::SetTrue),
		)
		.subcommand(Command::new("ci"))
		.subcommand(
			Command::new("doc").arg(
				Arg::new("open")
					.value_name("open")
					.index(1)
					.action(ArgAction::SetTrue),
			),
		)
		.subcommand(
			Command::new("bench").arg(
				Arg::new("build")
					.value_name("build")
					.index(1)
					.default_value(""),
			),
		)
		.subcommand(Command::new("install"))
		.subcommand(Command::new("validate"))
		.get_matches();

	if matches.get_flag("help") {
		print_help();
	}

	if matches.get_flag("version") {
		print_version();
	}

	if let Err(err) = dispatch(matches) {
		print_error(err);
		exit(EXIT_FAILURE);
	}
}

fn print_error(err: Error) {
	let mut chain = err.chain();

	// PANIC: First error is guaranteed to be present.
	eprintln!("error: {}", chain.next().unwrap());

	for err in chain {
		eprintln!("       {}", err);
	}
}

fn dispatch(matches: ArgMatches) -> Result<()> {
	match matches.subcommand() {
		Some(("validate", _)) => task::validate::run(),
		Some(("install", _)) => task::install::run(),
		Some(("ci", _)) => task::ci::run(),
		Some(("bench", _)) => task::bench::build(),
		Some(("doc", doc)) => {
			// PANIC: Should be safe to unwrap, because there is a default value
			let open = OpenDoc::from(doc.get_flag("open"));
			task::doc::run(open)
		}
		Some((_, _)) | None => print_help(),
	}
}

fn print_help() -> ! {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find xtask package version");

	let help_text = format!(
		"\
cargo {} {}
{}

USAGE:
    cargo {} [FLAGS] [<TASK>]

FLAGS:
    -h, --help            print help information
    -v, --version         print version information

TASKS:
    ci                    simulate a CI run locally
    doc [open]            generate docs for all crates in the workspace
    install               install hipcheck
    bench [build]         run time benchmarks to get duration on events
    validate              analyze workspace for expected configuration",
		env!("CARGO_PKG_NAME"),
		raw_version,
		env!("CARGO_PKG_DESCRIPTION"),
		env!("CARGO_BIN_NAME")
	);

	println!("{}", help_text);
	exit(EXIT_FAILURE);
}

fn print_version() -> ! {
	let version_text = crate_version!();
	println!("{}", version_text);
	exit(EXIT_SUCCESS);
}
