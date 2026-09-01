use anyhow::Result;
use nv_common::config::Config;

pub fn command() -> clap::Command {
    clap::Command::new("show")
        .about("Show normalized findings for one persisted assessment")
        .arg(assessment_id_argument())
        .arg(json_argument())
}

pub fn run(_config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    todo!("assessment show is not implemented")
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
        .help("Print normalized assessment findings as JSON")
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn show_accepts_an_assessment_id() {
        command()
            .try_get_matches_from(["show", "assessment-1", "--json"])
            .expect("assessment show should parse");
    }
}
