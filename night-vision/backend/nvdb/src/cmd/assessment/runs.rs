use anyhow::Result;
use nv_common::config::Config;

const DEFAULT_LIMIT: u64 = 10;

pub fn command() -> clap::Command {
    clap::Command::new("runs")
        .about("List persisted assessments for a package")
        .arg(
            clap::Arg::new("package")
                .long("package")
                .required(true)
                .value_name("PURL")
                .help("Package URL without a version"),
        )
        .arg(
            clap::Arg::new("limit")
                .long("limit")
                .value_name("N")
                .default_value(DEFAULT_LIMIT.to_string())
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Maximum number of assessments to list"),
        )
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("assessment runs is not implemented")
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print persisted assessments as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

    #[test]
    fn runs_requires_a_package() {
        let error = command()
            .try_get_matches_from(["runs"])
            .expect_err("missing package should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn runs_accepts_package_limit_and_json_output() {
        command()
            .try_get_matches_from([
                "runs",
                "--package",
                "pkg:npm/example",
                "--limit",
                "20",
                "--json",
            ])
            .expect("assessment runs should parse");
    }
}
