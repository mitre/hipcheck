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
            json_output(
                run.run.id,
                &run.run.status,
                run.run.target_purl.as_deref(),
                run.run.policy_id.as_deref(),
                run.run.policy_version.as_deref(),
                run.run.hipcheck_version.as_deref(),
                run.run.hipcheck_commit.as_deref(),
                run.run.policy_recommendation.as_deref(),
                run.run.created_at,
                findings,
            )
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

#[expect(
    clippy::too_many_arguments,
    reason = "the persisted assessment provenance is represented as flat database fields"
)]
fn json_output(
    id: i32,
    state: &str,
    target: Option<&str>,
    policy_id: Option<&str>,
    policy_version: Option<&str>,
    hipcheck_version: Option<&str>,
    hipcheck_commit: Option<&str>,
    recommendation: Option<&str>,
    created_at: impl serde::Serialize,
    findings: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "state": state,
        "target": target,
        "policy": { "id": policy_id, "version": policy_version },
        "hipcheck": { "version": hipcheck_version, "commit": hipcheck_commit },
        "recommendation": recommendation,
        "createdAt": created_at,
        "findings": findings,
    })
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
    use super::{command, json_output};

    #[test]
    fn show_accepts_an_assessment_id() {
        command()
            .try_get_matches_from(["show", "assessment-1", "--json"])
            .expect("assessment show should parse");
    }

    #[test]
    fn show_json_output_includes_assessment_provenance() {
        let output = json_output(
            7,
            "completed",
            Some("pkg:npm/example@1.2.3"),
            Some("night-vision"),
            Some("1"),
            Some("0.10.0"),
            Some("abc123"),
            Some("pass"),
            "2026-09-03T00:00:00Z",
            vec![serde_json::json!({"kind": "concern"})],
        );

        assert_eq!(output["target"], "pkg:npm/example@1.2.3");
        assert_eq!(output["policy"]["id"], "night-vision");
        assert_eq!(output["hipcheck"]["commit"], "abc123");
        assert_eq!(output["findings"][0]["kind"], "concern");
    }
}
