use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, hipcheck::storage::load_hipcheck_run_by_assessment_id, rt};
use uuid::Uuid;

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
        .parse::<Uuid>()?;
    let raw = matches.get_flag("raw-hipcheck");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let run = runtime.block_on(async {
        let db = db::connection(config).await?;
        load_hipcheck_run_by_assessment_id(&db, &id)
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
                &run.run.assessment_id,
                checks,
                concerns,
                findings,
                diagnostics_json(&run.run),
                raw.then_some(run.run.raw_json).flatten(),
            )
        );
    } else {
        println!("assessment {} evidence", run.run.assessment_id);
        print_diagnostics(&run.run);
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
    id: &str,
    checks: Vec<serde_json::Value>,
    concerns: Vec<serde_json::Value>,
    findings: Vec<serde_json::Value>,
    diagnostics: serde_json::Value,
    raw_hipcheck: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "checks": checks,
        "concerns": concerns,
        "findings": findings,
        "diagnostics": diagnostics,
        "rawHipcheck": raw_hipcheck,
    })
}

fn diagnostics_json(run: &nv_common::db::entities::hipcheck_runs::Model) -> serde_json::Value {
    serde_json::json!({
        "sourceRepositoryUrl": run.source_repository_url,
        "stdout": run.stdout,
        "stdoutTruncated": run.stdout_truncated,
        "stderr": run.stderr,
        "stderrTruncated": run.stderr_truncated,
        "exitStatus": run.exit_status,
        "errorKind": run.error_kind,
        "errorMessage": run.error_message,
        "retryable": run.retryable,
    })
}

fn print_diagnostics(run: &nv_common::db::entities::hipcheck_runs::Model) {
    println!(
        "source_repository_url: {}",
        run.source_repository_url.as_deref().unwrap_or("<none>")
    );
    println!(
        "exit_status: {}",
        run.exit_status
            .map_or_else(|| "<none>".to_owned(), |value| value.to_string())
    );
    println!(
        "error_kind: {}",
        run.error_kind.as_deref().unwrap_or("<none>")
    );
    println!(
        "error_message: {}",
        run.error_message.as_deref().unwrap_or("<none>")
    );
    println!(
        "retryable: {}",
        run.retryable
            .map_or_else(|| "<none>".to_owned(), |value| value.to_string())
    );
    println!("stdout_truncated: {}", run.stdout_truncated);
    println!("stdout:");
    println!("{}", run.stdout.as_deref().unwrap_or("<none>"));
    println!("stderr_truncated: {}", run.stderr_truncated);
    println!("stderr:");
    println!("{}", run.stderr.as_deref().unwrap_or("<none>"));
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

    const ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000007";

    #[test]
    fn evidence_accepts_raw_hipcheck_output() {
        command()
            .try_get_matches_from(["evidence", "assessment-1", "--raw-hipcheck", "--json"])
            .expect("assessment evidence should parse");
    }

    #[test]
    fn evidence_json_output_hides_raw_hipcheck_by_default() {
        let output = json_output(
            ASSESSMENT_ID,
            vec![serde_json::json!({"id": 1})],
            Vec::new(),
            vec![serde_json::json!({"kind": "concern"})],
            serde_json::json!({"errorMessage": "missing repository"}),
            None,
        );

        assert_eq!(output["id"], ASSESSMENT_ID);
        assert_eq!(output["checks"][0]["id"], 1);
        assert_eq!(output["findings"][0]["kind"], "concern");
        assert!(output["rawHipcheck"].is_null());
        assert_eq!(output["diagnostics"]["errorMessage"], "missing repository");
    }
}
