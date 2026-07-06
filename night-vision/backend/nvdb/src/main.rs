use anyhow::{Context as _, Result, bail};
use camino::Utf8PathBuf;
use nv_common::config::{Config, DEFAULT_CONFIG_FILE};
use secrecy::ExposeSecret as _;
use std::process::{Command, ExitCode};
use url::Url;

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
                .subcommand(
                    clap::Command::new("schema").about("Print the current database schema"),
                ),
        )
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("api") {
        todo!("api subcommand not yet implemented")
    }

    if let Some(db_matches) = matches.subcommand_matches("db")
        && let Some(_matches) = db_matches.subcommand_matches("schema")
    {
        let config_file = matches
            .get_one::<Utf8PathBuf>("config")
            .expect("config has a default value");
        let config = Config::parse(config_file)?;

        return print_database_schema(&config);
    }

    Ok(())
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
    use super::PostgresConnection;
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
}
