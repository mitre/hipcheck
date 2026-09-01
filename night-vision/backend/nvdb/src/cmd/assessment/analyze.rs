use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("analyze")
        .about("Run the configured assessment policy for a package version")
        .arg(purl_argument())
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("assessment analyze is not implemented")
}

fn purl_argument() -> clap::Arg {
    clap::Arg::new("purl")
        .required(true)
        .value_name("PURL")
        .help("Package URL for the version to assess")
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print the persisted assessment summary as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn analyze_accepts_a_purl() {
        command()
            .try_get_matches_from(["analyze", "pkg:npm/example@1.2.3", "--json"])
            .expect("assessment analyze should parse");
    }
}
