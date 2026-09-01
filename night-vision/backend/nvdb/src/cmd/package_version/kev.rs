use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("kev")
        .about("List KEV-linked CVE matches for a package version")
        .arg(
            clap::Arg::new("purl")
                .required(true)
                .value_name("PURL")
                .help("Package URL for the resolved package version"),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help("Print KEV-linked matches as JSON"),
        )
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("package-version kev is not implemented")
}

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

    #[test]
    fn kev_requires_a_purl() {
        let error = command()
            .try_get_matches_from(["kev"])
            .expect_err("missing PURL should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn kev_accepts_a_purl() {
        command()
            .try_get_matches_from(["kev", "pkg:npm/example@1.2.3", "--json"])
            .expect("package-version KEV lookup should parse");
    }
}
