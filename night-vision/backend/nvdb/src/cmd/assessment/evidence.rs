use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, hipcheck::storage::load_hipcheck_run, rt};

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

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let id = matches
        .get_one::<String>("assessment-id")
        .expect("required ID")
        .parse::<i32>()?;
    let raw = matches.get_flag("raw-hipcheck");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let run = runtime.block_on(async {
        let db = db::connection(config).await?;
        load_hipcheck_run(&db, id)
            .await?
            .context("assessment not found")
    })?;
    if matches.get_flag("json") {
        let checks = run
            .checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "id": check.id,
                    "state": check.state,
                    "effect": check.effect,
                    "summary": check.summary,
                })
            })
            .collect::<Vec<_>>();
        let concerns = run
            .concerns
            .iter()
            .map(|concern| {
                serde_json::json!({
                    "checkId": concern.check_id,
                    "kind": concern.kind,
                    "message": concern.message,
                    "details": concern.details,
                })
            })
            .collect::<Vec<_>>();
        let findings = run
            .findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "kind": finding.kind,
                    "effect": finding.effect,
                    "severity": finding.severity,
                    "summary": finding.summary,
                    "evidence": finding.evidence,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            json_output(
                run.run.id,
                checks,
                concerns,
                findings,
                raw.then_some(run.run.raw_json).flatten(),
            )
        );
    } else {
        println!("assessment {} evidence", run.run.id);
        println!("checks: {}", run.checks.len());
        for check in &run.checks {
            println!(
                "  {} {} {}: {}",
                check.id, check.state, check.effect, check.summary
            );
        }
        println!("concerns: {}", run.concerns.len());
        for concern in &run.concerns {
            println!(
                "  check {} {}: {}",
                concern.check_id, concern.kind, concern.message
            );
        }
        println!("findings: {}", run.findings.len());
        for finding in &run.findings {
            println!(
                "  {} {} {}: {}",
                finding.severity.as_deref().unwrap_or("<none>"),
                finding.effect,
                finding.kind,
                finding.summary
            );
        }
        if raw {
            println!("raw_hipcheck:");
            println!("{}", run.run.raw_json.as_deref().unwrap_or("<none>"));
        }
    }
    Ok(())
}

fn json_output(
    id: i32,
    checks: Vec<serde_json::Value>,
    concerns: Vec<serde_json::Value>,
    findings: Vec<serde_json::Value>,
    raw_hipcheck: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "checks": checks,
        "concerns": concerns,
        "findings": findings,
        "rawHipcheck": raw_hipcheck,
    })
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
    use super::{command, json_output};

    #[test]
    fn evidence_accepts_raw_hipcheck_output() {
        command()
            .try_get_matches_from(["evidence", "assessment-1", "--raw-hipcheck", "--json"])
            .expect("assessment evidence should parse");
    }

    #[test]
    fn evidence_json_output_hides_raw_hipcheck_by_default() {
        let output = json_output(
            7,
            vec![serde_json::json!({"id": 1})],
            Vec::new(),
            vec![serde_json::json!({"kind": "concern"})],
            None,
        );

        assert_eq!(output["id"], 7);
        assert_eq!(output["checks"][0]["id"], 1);
        assert_eq!(output["findings"][0]["kind"], "concern");
        assert!(output["rawHipcheck"].is_null());
    }
}
