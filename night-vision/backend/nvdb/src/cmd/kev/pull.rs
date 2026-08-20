use nv_common::config::Config;
use slog::{Logger, info};

use crate::destructive::DestructiveOperationToken;

pub fn command() -> clap::Command {
    clap::Command::new("pull")
        .about("Pull KEV Catalog")
        .arg(
            clap::Arg::new("destructive")
                .short('w')
                .long("destructive")
                .required(true)
                .action(clap::ArgAction::SetTrue)
                .help("Acknowledge this command may modify database state"),
        )
        .arg(
            clap::Arg::new("force")
                .short('f')
                .long("force")
                .action(clap::ArgAction::SetTrue)
                .help("Send unconditional HTTP request"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches, log: Logger) -> anyhow::Result<()> {
    let token = DestructiveOperationToken::new(matches);
    let force = matches.get_flag("force");
    pull(force, config, token, log)
}

fn pull(
    force: bool,
    config: &Config,
    _token: DestructiveOperationToken,
    log: Logger,
) -> anyhow::Result<()> {
    info!(log, "Pulling KEV Catalog");
    let rt = nv_common::rt::AsyncRuntime::new(config)?;

    let request_mode = if force {
        nv_common::kev::RequestMode::NoConditionalRequest
    } else {
        nv_common::kev::RequestMode::UseConditionalRequest
    };
    rt.block_on(nv_common::kev::run_fetch_kev(
        request_mode,
        config,
        log.clone(),
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

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
}
