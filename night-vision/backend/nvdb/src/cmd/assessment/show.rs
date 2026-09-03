use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, hipcheck::storage::load_hipcheck_run, rt};

pub fn command() -> clap::Command {
    clap::Command::new("show")
        .about("Show normalized findings for one persisted assessment")
        .arg(assessment_id_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let id = matches
        .get_one::<String>("assessment-id")
        .expect("required ID")
        .parse::<i32>()?;
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let run = runtime.block_on(async {
        let db = db::connection(config).await?;
        load_hipcheck_run(&db, id)
            .await?
            .context("assessment not found")
    })?;
    let findings = run
        .findings
        .iter()
        .map(|finding| {
            serde_json::json!({
                "kind": finding.kind,
                "effect": finding.effect,
                "severity": finding.severity,
                "summary": finding.summary,
            })
        })
        .collect::<Vec<_>>();
    if matches.get_flag("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": run.run.id,
                "state": run.run.status,
                "target": run.run.target_purl,
                "policy": {
                    "id": run.run.policy_id,
                    "version": run.run.policy_version,
                },
                "hipcheck": {
                    "version": run.run.hipcheck_version,
                    "commit": run.run.hipcheck_commit,
                },
                "recommendation": run.run.policy_recommendation,
                "createdAt": run.run.created_at,
                "findings": findings,
            })
        );
    } else {
        println!("assessment {}: {}", run.run.id, run.run.status);
        println!("target: {}", display_option(&run.run.target_purl));
        println!(
            "policy: {} ({})",
            display_option(&run.run.policy_id),
            display_option(&run.run.policy_version),
        );
        println!(
            "hipcheck: {} ({})",
            display_option(&run.run.hipcheck_version),
            display_option(&run.run.hipcheck_commit),
        );
        println!(
            "recommendation: {}",
            display_option(&run.run.policy_recommendation)
        );
        println!("created_at: {}", run.run.created_at);
        println!("findings: {}", findings.len());
        for finding in findings {
            println!("{finding}");
        }
    }
    Ok(())
}

fn display_option(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("<none>")
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
