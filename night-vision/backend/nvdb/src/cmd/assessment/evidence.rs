use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("evidence")
        .about("Inspect evidence retained for one persisted assessment")
        .arg(assessment_id_argument())
        .arg(
            clap::Arg::new("raw-hipcheck")
                .long("raw-hipcheck")
                .action(clap::ArgAction::SetTrue)
                .help("Include retained raw Hipcheck JSON output"),
        )
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("assessment evidence is not implemented")
}

fn assessment_id_argument() -> clap::Arg {
    clap::Arg::new("assessment-id")
        .required(true)
        .value_name("ASSESSMENT-ID")
        .help("Persisted assessment identifier")
}

fn json_argument() -> clap::Arg {
    clap::Arg::new("json")
        .long("json")
        .action(clap::ArgAction::SetTrue)
        .help("Print assessment evidence as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn evidence_accepts_raw_hipcheck_output() {
        command()
            .try_get_matches_from(["evidence", "assessment-1", "--raw-hipcheck", "--json"])
            .expect("assessment evidence should parse");
    }
}
