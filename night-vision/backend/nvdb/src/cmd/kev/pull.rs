use nv_common::config::Config;
use slog::Logger;

pub const DEPRECATION_WARNING: &str =
	"warning: `kev pull` is deprecated; use `kev sync --destructive` instead";

pub fn command() -> clap::Command {
	clap::Command::new("pull")
		.hide(true)
		.args(crate::cmd::kev::sync::command().get_arguments())
}

pub fn run(config: &Config, matches: &clap::ArgMatches, log: Logger) -> anyhow::Result<()> {
	eprintln!("{DEPRECATION_WARNING}");
	crate::cmd::kev::sync::run(config, matches, log)
}

#[cfg(test)]
mod tests {
	use super::command;
	use clap::error::ErrorKind;

	#[test]
	fn kev_pull_is_hidden_from_help() {
		assert!(command().is_hide_set());
	}

	#[test]
	fn kev_pull_requires_destructive_flag() {
		let error = command()
			.try_get_matches_from(["pull"])
			.expect_err("missing destructive flag should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn kev_pull_accepts_destructive_flag() {
		command()
			.try_get_matches_from(["pull", "--destructive"])
			.expect("destructive flag should parse");
	}

	#[test]
	fn kev_pull_accepts_force_with_destructive_flag() {
		let matches = command()
			.try_get_matches_from(["pull", "--destructive", "--force"])
			.expect("destructive and force flags should parse");

		assert!(matches.get_flag("force"));
	}

	#[test]
	fn kev_pull_warning_recommends_sync() {
		assert_eq!(
			super::DEPRECATION_WARNING,
			"warning: `kev pull` is deprecated; use `kev sync --destructive` instead"
		);
	}
}
