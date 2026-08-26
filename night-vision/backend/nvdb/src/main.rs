use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Args as _, FromArgMatches as _};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use nv_common::config::{Config, DEFAULT_CONFIG_FILE};
use nv_common::log::logger;
use std::{fmt::Write as _, process::ExitCode};

mod cmd;
mod destructive;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{}", format_error_report(&e));
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn format_error_report(error: &anyhow::Error) -> String {
    let mut report = error.to_string();
    let causes = error.chain().skip(1).collect::<Vec<_>>();

    if !causes.is_empty() {
        report.push_str("\n\nCaused by:\n");

        for (index, cause) in causes.iter().enumerate() {
            let _ = writeln!(
                report,
                "    {index}: {}",
                format_cause_without_duplicated_source(*cause)
            );
        }
    }

    report
}

fn format_cause_without_duplicated_source(error: &(dyn std::error::Error + 'static)) -> String {
    let message = error.to_string();
    let Some(source) = error.source() else {
        return message;
    };

    let source_message = source.to_string();
    let duplicated_source = format!(": {source_message}");

    message
        .strip_suffix(&duplicated_source)
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(&message)
        .to_owned()
}

fn run() -> Result<()> {
    let matches = command().get_matches();
    let verbosity_filter = Verbosity::<InfoLevel>::from_arg_matches(&matches)
        .expect("verbose and quiet args are always present and default to 0")
        .filter();
    let log = logger(verbosity_filter);

    if let Some(npm_matches) = matches.subcommand_matches("npm") {
        return cmd::npm::run(npm_matches);
    }
    let config_file = matches
        .get_one::<Utf8PathBuf>("config")
        .expect("config has a default value");
    let config = Config::parse(config_file)?;
    if let Some(sub_matches) = matches.subcommand_matches("api")
        && let Some(_sub_matches) = sub_matches.subcommand_matches("health")
    {
        return cmd::api::health::run(&config);
    }
    if let Some(db_matches) = matches.subcommand_matches("db") {
        if let Some(_matches) = db_matches.subcommand_matches("ping") {
            return cmd::db::ping::run(&config);
        }

        if let Some(_matches) = db_matches.subcommand_matches("schema") {
            return cmd::db::schema::run(&config);
        }

        if let Some(entity_matches) = db_matches.subcommand_matches("entity") {
            return cmd::db::entity::run(&config, entity_matches);
        }

        if let Some(migrate_matches) = db_matches.subcommand_matches("migrate") {
            return cmd::db::migrate::run(&config, migrate_matches);
        }
    }

    if let Some(cve_matches) = matches.subcommand_matches("cve") {
        return cmd::cve::run(&config, cve_matches);
    }

    if let Some(kev_matches) = matches.subcommand_matches("kev") {
        if let Some(config_matches) = kev_matches.subcommand_matches("config") {
            return cmd::kev::config::run(&config, config_matches);
        } else if let Some(doctor_matches) = kev_matches.subcommand_matches("doctor") {
            return cmd::kev::doctor::run(&config, doctor_matches);
        } else if let Some(pull_matches) = kev_matches.subcommand_matches("pull") {
            return cmd::kev::pull::run(&config, pull_matches, log);
        } else if let Some(list_matches) = kev_matches.subcommand_matches("list") {
            return cmd::kev::list::run(&config, list_matches);
        } else if let Some(record_matches) = kev_matches.subcommand_matches("record") {
            return cmd::kev::record::run(&config, record_matches);
        } else if let Some(run_matches) = kev_matches.subcommand_matches("run") {
            return cmd::kev::run::run(&config, run_matches);
        } else if let Some(runs_matches) = kev_matches.subcommand_matches("runs") {
            return cmd::kev::runs::run(&config, runs_matches);
        } else if let Some(stats_matches) = kev_matches.subcommand_matches("stats") {
            return cmd::kev::stats::run(&config, stats_matches);
        } else if let Some(status_matches) = kev_matches.subcommand_matches("status") {
            return cmd::kev::status::run(&config, status_matches);
        }
    }

    Ok(())
}

fn command() -> clap::Command {
    let command = clap::Command::new("nvdb")
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
                .arg_required_else_help(true)
                .subcommand(cmd::api::health::command()),
        )
        .subcommand(
            clap::Command::new("db")
                .about("Manage the database")
                .arg_required_else_help(true)
                .subcommand(cmd::db::entity::command())
                .subcommand(cmd::db::ping::command())
                .subcommand(cmd::db::schema::command())
                .subcommand(cmd::db::migrate::command()),
        )
        .subcommand(cmd::cve::command())
        .subcommand(cmd::npm::command())
        .subcommand(
            clap::Command::new("kev")
                .about("Manage KEV data")
                .arg_required_else_help(true)
                .subcommand(cmd::kev::config::command())
                .subcommand(cmd::kev::doctor::command())
                .subcommand(cmd::kev::list::command())
                .subcommand(cmd::kev::pull::command())
                .subcommand(cmd::kev::record::command())
                .subcommand(cmd::kev::run::command())
                .subcommand(cmd::kev::runs::command())
                .subcommand(cmd::kev::stats::command())
                .subcommand(cmd::kev::status::command()),
        );
    Verbosity::<InfoLevel>::augment_args(command)
}

#[cfg(test)]
mod tests {
    use super::format_error_report;
    use anyhow::anyhow;

    #[test]
    fn error_report_prints_causes_on_separate_lines() {
        let error = anyhow!("leaf").context("middle").context("top");

        let report = format_error_report(&error);

        assert!(report.starts_with("top\n\nCaused by:\n"));
        assert!(report.contains("\n    0: middle\n"));
        assert!(report.contains("\n    1: leaf"));
        assert!(!report.contains("top: middle: leaf"));
    }

    #[test]
    fn error_report_does_not_duplicate_source_in_cause_display() {
        let error = anyhow!(ExecutionError(DatabaseError)).context("failed to sync CVE List data");

        let report = format_error_report(&error);

        assert_eq!(
            report,
            "failed to sync CVE List data\n\nCaused by:\n    0: Execution Error\n    1: error returned from database\n"
        );
    }

    #[derive(Debug)]
    struct ExecutionError(DatabaseError);

    impl std::fmt::Display for ExecutionError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Execution Error: {}", self.0)
        }
    }

    impl std::error::Error for ExecutionError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[derive(Debug)]
    struct DatabaseError;

    impl std::fmt::Display for DatabaseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "error returned from database")
        }
    }

    impl std::error::Error for DatabaseError {}
}
