use anyhow::{Context as _, Result, bail};
use nv_common::config::Config;
use secrecy::ExposeSecret as _;
use std::process::Command;

use crate::destructive::DestructiveOperationToken;

pub fn command() -> clap::Command {
	clap::Command::new("migrate")
		.about("Run database migrations")
		.arg(
			clap::Arg::new("destructive")
				.short('w')
				.long("destructive")
				.required(true)
				.action(clap::ArgAction::SetTrue)
				.help("Acknowledge this command may modify database state"),
		)
		.arg(
			clap::Arg::new("args")
				.value_name("ARGS")
				.num_args(0..)
				.allow_hyphen_values(true)
				.trailing_var_arg(true)
				.help("Arguments to pass to sea-orm-cli migrate"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let token = DestructiveOperationToken::new(matches);
	let args = matches
		.get_many::<String>("args")
		.into_iter()
		.flatten()
		.map(String::as_str);

	run_database_migrations(config, token, args)
}

fn run_database_migrations<'a>(
	config: &Config,
	_token: DestructiveOperationToken,
	args: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
	let status = sea_orm_migrate_command(config, args)
		.status()
		.context("failed to run sea-orm-cli migrate")?;

	if !status.success() {
		bail!("sea-orm-cli migrate failed");
	}

	Ok(())
}

fn sea_orm_migrate_command<'a>(
	config: &Config,
	args: impl IntoIterator<Item = &'a str>,
) -> Command {
	sea_orm_migrate_command_with_database_url(config.database_connection().expose_secret(), args)
}

fn sea_orm_migrate_command_with_database_url<'a>(
	database_url: &str,
	args: impl IntoIterator<Item = &'a str>,
) -> Command {
	let mut command = Command::new("sea-orm-cli");
	command
		.arg("migrate")
		.args(args)
		.env("DATABASE_URL", database_url);
	command
}

#[cfg(test)]
mod tests {
	use super::{command, sea_orm_migrate_command_with_database_url};
	use clap::error::ErrorKind;
	use std::collections::BTreeMap;

	#[test]
	fn database_migrate_requires_destructive_flag() {
		let error = command()
			.try_get_matches_from(["migrate", "up"])
			.expect_err("missing destructive flag should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn database_migrate_accepts_trailing_sea_orm_args() {
		let matches = command()
			.try_get_matches_from(["migrate", "-w", "up", "-n", "2"])
			.expect("migrate args should parse");
		let args = matches
			.get_many::<String>("args")
			.expect("migrate args should be present")
			.map(String::as_str)
			.collect::<Vec<_>>();

		assert_eq!(args, ["up", "-n", "2"]);
	}

	#[test]
	fn sea_orm_migrate_command_sets_database_url_and_forwards_args() {
		let command = sea_orm_migrate_command_with_database_url(
			"postgres://user:password@localhost:5432/nv",
			["up", "-n", "2"],
		);
		let args = command
			.get_args()
			.map(|arg| arg.to_string_lossy().into_owned())
			.collect::<Vec<_>>();
		let env = command
			.get_envs()
			.filter_map(|(key, value)| {
				value.map(|value| {
					(
						key.to_string_lossy().into_owned(),
						value.to_string_lossy().into_owned(),
					)
				})
			})
			.collect::<BTreeMap<_, _>>();

		assert_eq!(args, ["migrate", "up", "-n", "2"]);
		assert_eq!(
			env.get("DATABASE_URL"),
			Some(&"postgres://user:password@localhost:5432/nv".to_owned())
		);
	}
}
