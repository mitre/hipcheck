use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db,
    hipcheck::{
        assessment::{execute_queued_assessment, queue_assessment},
        storage::load_hipcheck_run,
    },
    rt,
};

pub fn command() -> clap::Command {
    clap::Command::new("analyze")
        .about("Run the configured assessment policy for a package version")
        .arg(purl_argument())
        .arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let purl = matches.get_one::<String>("purl").expect("required PURL");
    eprintln!("queuing assessment for {purl}");
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    let run = runtime.block_on(async {
        let db = db::connection(config).await?;
        let queued = queue_assessment(&db, purl).await?;
        eprintln!(
            "running configured Hipcheck policy for assessment {}",
            queued.id
        );
        execute_queued_assessment(&db, &queued, &config.hipcheck_runner_config()).await?;
        load_hipcheck_run(&db, queued.id)
            .await?
            .context("assessment disappeared after persistence")
    })?;
    eprintln!("completed assessment {}", run.run.id);
    if matches.get_flag("json") {
        println!(
            "{}",
            json_output(
                run.run.id,
                purl,
                &run.run.status,
                run.run.policy_recommendation.as_deref(),
                run.findings.len(),
            )
        );
    } else {
        println!(
            "assessment {}\ntarget: {}\nstate: {}\nrecommendation: {}\nfindings: {}",
            run.run.id,
            purl,
            run.run.status,
            run.run.policy_recommendation.as_deref().unwrap_or("n/a"),
            run.findings.len()
        );
    }
    Ok(())
}

fn json_output(
    id: i32,
    purl: &str,
    state: &str,
    recommendation: Option<&str>,
    finding_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "target": purl,
        "state": state,
        "recommendation": recommendation,
        "findingCount": finding_count,
    })
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
    use super::{command, json_output};

    #[test]
    fn analyze_accepts_a_purl() {
        command()
            .try_get_matches_from(["analyze", "pkg:npm/example@1.2.3", "--json"])
            .expect("assessment analyze should parse");
    }

    #[test]
    fn analyze_json_output_preserves_the_completed_assessment_summary() {
        assert_eq!(
            json_output(7, "pkg:npm/example@1.2.3", "completed", Some("pass"), 2),
            serde_json::json!({
                "id": 7,
                "target": "pkg:npm/example@1.2.3",
                "state": "completed",
                "recommendation": "pass",
                "findingCount": 2,
            })
        );
    }
}
