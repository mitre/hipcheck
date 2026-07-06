use anyhow::{Context as _, Result, bail};
use camino::Utf8PathBuf;
use nv_common::config::{Config, DEFAULT_CONFIG_FILE};
use secrecy::ExposeSecret as _;
use std::process::{Command, ExitCode};
use url::Url;

use destructive::DestructiveOperationToken;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let matches = command().get_matches();

    if let Some(_matches) = matches.subcommand_matches("api") {
        todo!("api subcommand not yet implemented")
    }

    let config_file = matches
        .get_one::<Utf8PathBuf>("config")
        .expect("config has a default value");
    let config = Config::parse(config_file)?;

    if let Some(db_matches) = matches.subcommand_matches("db") {
        if let Some(_matches) = db_matches.subcommand_matches("schema") {
            return print_database_schema(&config);
        }

        if let Some(migrate_matches) = db_matches.subcommand_matches("migrate") {
            let token = DestructiveOperationToken::new(migrate_matches);
            let args = migrate_matches
                .get_many::<String>("args")
                .into_iter()
                .flatten()
                .map(String::as_str);

            return run_database_migrations(&config, token, args);
        }
    }

    Ok(())
}

fn command() -> clap::Command {
    clap::Command::new("nvdb")
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
                .subcommand(clap::Command::new("schema").about("Print the current database schema"))
                .subcommand(
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
                        ),
                ),
        )
}

fn print_database_schema(config: &Config) -> Result<()> {
    let database_connection =
        PostgresConnection::parse(config.database_connection().expose_secret())?;
    let mut command = Command::new("pg_dump");
    database_connection.apply_to_command(&mut command);

    let output = command
        .arg("--schema-only")
        .arg("--no-owner")
        .arg("--no-privileges")
        .output()
        .context("failed to run pg_dump")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pg_dump failed: {}", stderr.trim());
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));

    Ok(())
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

mod destructive {
    #[must_use]
    pub struct DestructiveOperationToken {
        _private: (),
    }

    impl DestructiveOperationToken {
        pub fn new(matches: &clap::ArgMatches) -> Self {
            assert!(
                matches.get_flag("destructive"),
                "destructive operation token requires --destructive"
            );
            Self { _private: () }
        }
    }
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

struct PostgresConnection<'a> {
    url: Url,
    connection_string: &'a str,
}

impl<'a> PostgresConnection<'a> {
    fn parse(connection_string: &'a str) -> Result<Self> {
        let url =
            Url::parse(connection_string).context("failed to parse database connection URL")?;
        match url.scheme() {
            "postgres" | "postgresql" => Ok(Self {
                url,
                connection_string,
            }),
            scheme => bail!("unsupported database connection scheme '{scheme}'"),
        }
    }

    fn apply_to_command(&self, command: &mut Command) {
        command
            .env_remove("PGDATABASE")
            .env_remove("PGHOST")
            .env_remove("PGPORT")
            .env_remove("PGUSER")
            .env_remove("PGPASSWORD");

        command.env("PGDATABASE", self.database_name());

        if let Some(host) = self.url.host_str() {
            command.env("PGHOST", host);
        }

        if let Some(port) = self.url.port() {
            command.env("PGPORT", port.to_string());
        }

        if !self.url.username().is_empty() {
            command.env("PGUSER", self.url.username());
        }

        if let Some(password) = self.url.password() {
            command.env("PGPASSWORD", password);
        }
    }

    fn database_name(&self) -> &str {
        self.url
            .path()
            .strip_prefix('/')
            .filter(|database_name| !database_name.is_empty())
            .unwrap_or(self.connection_string)
    }
}

#[cfg(test)]
mod tests {
    use super::{PostgresConnection, command, sea_orm_migrate_command_with_database_url};
    use clap::error::ErrorKind;
    use std::{collections::BTreeMap, ffi::OsStr, process::Command};

    #[test]
    fn postgres_connection_applies_url_components_to_pg_env() {
        let connection = PostgresConnection::parse("postgres://user:password@localhost:5432/nv")
            .expect("connection URL should parse");
        let mut command = Command::new("pg_dump");

        connection.apply_to_command(&mut command);

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

        assert_eq!(env.get("PGDATABASE"), Some(&"nv".to_owned()));
        assert_eq!(env.get("PGHOST"), Some(&"localhost".to_owned()));
        assert_eq!(env.get("PGPORT"), Some(&"5432".to_owned()));
        assert_eq!(env.get("PGUSER"), Some(&"user".to_owned()));
        assert_eq!(env.get("PGPASSWORD"), Some(&"password".to_owned()));
        assert!(
            !command
                .get_args()
                .any(|arg| arg == OsStr::new("postgres://user:password@localhost:5432/nv"))
        );
    }

    #[test]
    fn database_migrate_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["nvdb", "db", "migrate", "up"])
            .expect_err("missing destructive flag should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn database_migrate_accepts_trailing_sea_orm_args() {
        let matches = command()
            .try_get_matches_from(["nvdb", "db", "migrate", "-w", "up", "-n", "2"])
            .expect("migrate args should parse");
        let db_matches = matches
            .subcommand_matches("db")
            .expect("db subcommand should be present");
        let migrate_matches = db_matches
            .subcommand_matches("migrate")
            .expect("migrate subcommand should be present");
        let args = migrate_matches
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
