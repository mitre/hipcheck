use hc_common::{CheckKind, Result};
use hc_core::*;
use std::ffi::OsString;
use tempfile::tempdir;

// Don't run these tests by default, since they're slow.

/// Check if Hipcheck can run against its own repository.
#[test]
#[ignore]
fn hc_can_run() {
	// Run Hipcheck on its own repo.
	check_can_run(".").unwrap();
}

/// Check if Hipcheck can run against an empty repository.
#[test]
#[should_panic(
	expected = "called `Result::unwrap()` on an `Err` value: can't get head commit for local source"
)]
#[ignore]
fn hc_properly_errors_on_empty_repo() {
	// Create an empty git repo.
	let dir = tempdir().unwrap();
	let _ = duct::cmd!("git", "init", dir.path());
	let result = check_can_run(dir.path());
	dir.close().unwrap();
	result.unwrap();
}

fn check_can_run<S: Into<OsString>>(repo: S) -> Result<()> {
	let shell = {
		// Silent mode to ensure no output at all.
		let verbosity = Verbosity::Silent;
		let color_choice = ColorChoice::Never;

		Shell::new(
			Output::stdout(color_choice),
			Output::stderr(color_choice),
			verbosity,
		)
	};

	let check = Check {
		check_type: CheckType::RepoSource,
		check_value: repo.into(),
		parent_command_value: CheckKind::Repo.name().to_string(),
	};

	let config_path = None;
	let data_path = None;
	let home_dir = None;
	let format = Format::Human;
	let raw_version = "0.0.0";

	// Get the result, drop the `Shell`.
	let (_, result) = run_with_shell(
		shell,
		check,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	);

	// Ignore contents of Result::Ok
	result.map(|_| ())
}
