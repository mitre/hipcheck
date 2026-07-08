use anyhow::{Context as _, Result, bail};
use nv_common::config::Config;
use secrecy::ExposeSecret as _;
use std::{path::Path, process::Command};

pub fn command() -> clap::Command {
    clap::Command::new("entity")
        .about("Manage SeaORM entities")
        .arg_required_else_help(true)
        .subcommand(clap::Command::new("generate").about("Generate SeaORM entities"))
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    if let Some(_matches) = matches.subcommand_matches("generate") {
        return generate_entities(config);
    }

    Ok(())
}

fn generate_entities(config: &Config) -> Result<()> {
    let status = sea_orm_generate_entity_command(config.database_connection().expose_secret())
        .status()
        .context("failed to run sea-orm-cli generate entity")?;

    if !status.success() {
        bail!("sea-orm-cli generate entity failed");
    }

    Ok(())
}

fn sea_orm_generate_entity_command(database_url: &str) -> Command {
    sea_orm_generate_entity_command_with_output_dir(database_url, entity_output_dir())
}

fn entity_output_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nv-common/src/db/entities")
}

fn sea_orm_generate_entity_command_with_output_dir(
    database_url: &str,
    output_dir: impl AsRef<Path>,
) -> Command {
    let mut command = Command::new("sea-orm-cli");
    command
        .arg("generate")
        .arg("entity")
        .arg("--ignore-tables")
        .arg("seaql_migrations")
        .arg("--date-time-crate")
        .arg("chrono")
        .arg("--database-schema")
        .arg("public")
        .arg("--output-dir")
        .arg(output_dir.as_ref())
        .env("DATABASE_URL", database_url);
    command
}

#[cfg(test)]
mod tests {
    use super::{command, sea_orm_generate_entity_command_with_output_dir};
    use std::{collections::BTreeMap, ffi::OsStr, path::PathBuf};

    #[test]
    fn entity_command_requires_subcommand() {
        let error = command()
            .try_get_matches_from(["entity"])
            .expect_err("missing subcommand should fail");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn entity_command_accepts_generate_subcommand() {
        command()
            .try_get_matches_from(["entity", "generate"])
            .expect("generate subcommand should parse");
    }

    #[test]
    fn sea_orm_generate_entity_command_sets_database_url_and_defaults() {
        let output_dir = PathBuf::from("../nv-common/src/db/entities");
        let command = sea_orm_generate_entity_command_with_output_dir(
            "postgres://user:password@localhost:5432/nv",
            &output_dir,
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

        assert_eq!(
            args,
            [
                "generate",
                "entity",
                "--ignore-tables",
                "seaql_migrations",
                "--date-time-crate",
                "chrono",
                "--database-schema",
                "public",
                "--output-dir",
                "../nv-common/src/db/entities"
            ]
        );
        assert_eq!(
            env.get("DATABASE_URL"),
            Some(&"postgres://user:password@localhost:5432/nv".to_owned())
        );
        assert!(
            !command
                .get_args()
                .any(|arg| arg == OsStr::new("postgres://user:password@localhost:5432/nv"))
        );
    }
}
