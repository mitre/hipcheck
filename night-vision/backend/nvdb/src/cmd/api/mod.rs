//! Direct, typed access to the configured Night Vision REST API.

use anyhow::{Context as _, Result, anyhow, bail};
use camino::Utf8PathBuf;
use nv_common::{config::Config, rt::AsyncRuntime};
use nv_server_client::{Client, types};
use secrecy::ExposeSecret as _;
use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use uuid::Uuid;

pub mod assessment;

const USER_AGENT: &str = "nvdb";

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let runtime = AsyncRuntime::new(config).context("failed to create async runtime")?;
    let base_url = format!("http://{}", config.server_address);

    if let Some(health) = matches.subcommand_matches("health")
        && health.subcommand_matches("diagnostics").is_none()
    {
        return runtime.block_on(async {
            let response = client(&base_url)?
                .health()
                .send()
                .await
                .map_err(|error| request_error("GET /health", error))?;
            print_json(response.into_inner())
        });
    }
    if let Some(health) = matches.subcommand_matches("health")
        && health.subcommand_matches("diagnostics").is_some()
    {
        return runtime.block_on(async {
            let response = diagnostics_client(config, &base_url)?
                .health_diagnostics()
                .send()
                .await
                .map_err(|error| request_error("GET /health/diagnostics", error))?;
            print_json(response.into_inner())
        });
    }
    if let Some(package_source) = matches.subcommand_matches("package-sources") {
        if let Some(submit) = package_source.subcommand_matches("submit") {
            let file = required_path(submit, "package-json-file")?;
            let body = package_source_body(file)?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .post_package_source()
                    .body(body)
                    .send()
                    .await
                    .map_err(|error| request_error("POST /package-sources", error))?;
                print_json(response.into_inner())
            });
        }
        if let Some(get) = package_source.subcommand_matches("get") {
            let id = required_uuid(get, "id")?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .get_package_source()
                    .id(id)
                    .send()
                    .await
                    .map_err(|error| request_error("GET /package-sources/{id}", error))?;
                print_json(response.into_inner())
            });
        }
    }
    if let Some(assessments) = matches.subcommand_matches("assessments") {
        if let Some(submit) = assessments.subcommand_matches("submit") {
            let body = types::PostAssessmentBody {
                affected_purl: required_string(submit, "affected-purl")?.to_owned(),
                target_purl: required_string(submit, "target-purl")?.to_owned(),
            };
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .post_assessment()
                    .body(body)
                    .send()
                    .await
                    .map_err(|error| request_error("POST /assessments", error))?;
                print_json(response.into_inner())
            });
        }
        if let Some(get) = assessments.subcommand_matches("get") {
            let id = required_uuid(get, "id")?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .get_assessment()
                    .id(id)
                    .send()
                    .await
                    .map_err(|error| request_error("GET /assessments/{id}", error))?;
                print_json(response.into_inner())
            });
        }
        if let Some(evidence) = assessments.subcommand_matches("evidence") {
            let id = required_uuid(evidence, "id")?;
            let include_raw_hipcheck = evidence.get_flag("include-raw-hipcheck");
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .get_assessment_evidence()
                    .id(id)
                    .include_raw_hipcheck(include_raw_hipcheck)
                    .send()
                    .await
                    .map_err(|error| request_error("GET /assessments/{id}/evidence", error))?;
                print_json(response.into_inner())
            });
        }
    }
    if let Some(upgrades) = matches.subcommand_matches("upgrade-assessments") {
        if let Some(submit) = upgrades.subcommand_matches("submit") {
            let body: types::UpgradeAssessmentInput = read_json_file(
                required_path(submit, "request-json-file")?,
                "upgrade assessment request",
            )?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .post_upgrade_assessment()
                    .body(body)
                    .send()
                    .await
                    .map_err(|error| request_error("POST /upgrade-assessments", error))?;
                print_json(response.into_inner())
            });
        }
        if let Some(get) = upgrades.subcommand_matches("get") {
            let id = required_uuid(get, "id")?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .get_upgrade_assessment()
                    .id(id)
                    .send()
                    .await
                    .map_err(|error| request_error("GET /upgrade-assessments/{id}", error))?;
                print_json(response.into_inner())
            });
        }
        if let Some(result) = upgrades.subcommand_matches("result") {
            let id = required_uuid(result, "id")?;
            return runtime.block_on(async {
                let response = client(&base_url)?
                    .get_upgrade_assessment_result()
                    .id(id)
                    .send()
                    .await
                    .map_err(|error| {
                        request_error("GET /upgrade-assessments/{id}/result", error)
                    })?;
                print_json(response.into_inner())
            });
        }
    }
    bail!("missing API command")
}

/// Build an unauthenticated API client for the configured service.
pub fn client(base_url: &str) -> Result<Client> {
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("failed to create API client")?;
    Ok(Client::new_with_client(base_url, http))
}

fn diagnostics_client(config: &Config, base_url: &str) -> Result<Client> {
    let token = config
        .health_diagnostics_token()
        .ok_or_else(|| anyhow!("health diagnostics requires health-diagnostics-token-file"))?;
    let authorization =
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            .context("health diagnostics token is not valid for an HTTP header")?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::AUTHORIZATION, authorization);
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .context("failed to create diagnostics API client")?;
    Ok(Client::new_with_client(base_url, http))
}

fn request_error<E: std::fmt::Display>(operation: &str, error: E) -> anyhow::Error {
    anyhow!("{operation} failed: {error}")
}

fn print_json<T: Serialize>(value: T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn required_string<'a>(matches: &'a clap::ArgMatches, name: &str) -> Result<&'a str> {
    matches
        .get_one::<String>(name)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("missing required argument {name}"))
}

fn required_uuid(matches: &clap::ArgMatches, name: &str) -> Result<Uuid> {
    required_string(matches, name)?
        .parse()
        .context("ID must be a UUID")
}

fn required_path<'a>(matches: &'a clap::ArgMatches, name: &str) -> Result<&'a Utf8PathBuf> {
    matches
        .get_one(name)
        .ok_or_else(|| anyhow!("missing required argument {name}"))
}

fn read_json_file<T: DeserializeOwned>(path: &Utf8PathBuf, description: &str) -> Result<T> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read {description} file {path}"))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {description} JSON from {path}"))
}

fn package_source_body(path: &Utf8PathBuf) -> Result<types::PostPackageSourceBody> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("package source path must name a file: {path}"))?
        .to_owned();
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read package source file {path} as UTF-8"))?;
    Ok(types::PostPackageSourceBody {
        file_name,
        contents,
    })
}

pub fn command() -> clap::Command {
    clap::Command::new("api")
        .about("Interact directly with the REST API")
        .arg_required_else_help(true)
        .subcommand(
            clap::Command::new("health")
                .about("Get API liveness")
                .subcommand(
                    clap::Command::new("diagnostics")
                        .about("Get authenticated API health diagnostics"),
                ),
        )
        .subcommand(
            clap::Command::new("package-sources")
                .arg_required_else_help(true)
                .subcommand(
                    clap::Command::new("submit")
                        .arg(path_argument("package-json-file", "PACKAGE_JSON_FILE")),
                )
                .subcommand(clap::Command::new("get").arg(uuid_argument())),
        )
        .subcommand(
            clap::Command::new("assessments")
                .arg_required_else_help(true)
                .subcommand(
                    clap::Command::new("submit")
                        .arg(required_option("affected-purl", "AFFECTED_PURL"))
                        .arg(required_option("target-purl", "TARGET_PURL")),
                )
                .subcommand(clap::Command::new("get").arg(uuid_argument()))
                .subcommand(
                    clap::Command::new("evidence").arg(uuid_argument()).arg(
                        clap::Arg::new("include-raw-hipcheck")
                            .long("include-raw-hipcheck")
                            .action(clap::ArgAction::SetTrue),
                    ),
                ),
        )
        .subcommand(
            clap::Command::new("upgrade-assessments")
                .arg_required_else_help(true)
                .subcommand(
                    clap::Command::new("submit")
                        .arg(path_argument("request-json-file", "REQUEST_JSON_FILE")),
                )
                .subcommand(clap::Command::new("get").arg(uuid_argument()))
                .subcommand(clap::Command::new("result").arg(uuid_argument())),
        )
}

fn uuid_argument() -> clap::Arg {
    clap::Arg::new("id").value_name("ID").required(true)
}
fn path_argument(name: &'static str, value_name: &'static str) -> clap::Arg {
    clap::Arg::new(name)
        .value_name(value_name)
        .required(true)
        .value_parser(clap::value_parser!(Utf8PathBuf))
}
fn required_option(name: &'static str, value_name: &'static str) -> clap::Arg {
    clap::Arg::new(name)
        .long(name)
        .value_name(value_name)
        .required(true)
}

#[cfg(test)]
mod tests {
    use super::{command, package_source_body, run};
    use camino::Utf8PathBuf;
    use httpmock::prelude::*;
    use nv_common::config::Config;
    use std::fmt::Write as _;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    const ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000007";
    const PACKAGE_SOURCE_ID: &str = "0198f30e-2bfa-7000-8000-000000000008";
    const UPGRADE_ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000009";

    #[derive(Default)]
    struct TempFiles {
        paths: Vec<Utf8PathBuf>,
    }

    impl TempFiles {
        fn write(&mut self, description: &str, contents: impl AsRef<[u8]>) -> Utf8PathBuf {
            let path = Utf8PathBuf::from_path_buf(std::env::temp_dir())
                .expect("temporary directory path is UTF-8")
                .join(format!(
                    "night-vision-api-{description}-{}",
                    uuid::Uuid::now_v7()
                ));
            fs::write(&path, contents).expect("write temporary fixture");
            self.paths.push(path.clone());
            path
        }
    }

    impl Drop for TempFiles {
        fn drop(&mut self) {
            for path in &self.paths {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn test_config(base_url: &str, diagnostics_token: Option<&str>) -> (Config, TempFiles) {
        let mut files = TempFiles::default();
        let token_path = diagnostics_token.map(|token| {
            let path = files.write("diagnostics-token", format!("{token}\n"));
            #[cfg(unix)]
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("restrict token fixture permissions");
            path
        });
        let server_address = base_url
            .strip_prefix("http://")
            .expect("mock server must use HTTP");
        let mut config = format!(
            "server-address = {server_address}\n\
             cve-list-checkout-path = /tmp/night-vision-api-test-cvelistV5\n\
             database-connection = sqlite::memory:\n"
        );
        if let Some(path) = token_path {
            writeln!(config, "health-diagnostics-token-file = {path}").expect("write test config");
        }
        let config_path = files.write("config.spookey", config);
        let config = Config::parse(&config_path).expect("test config parses");
        (config, files)
    }

    fn api_matches(args: &[&str]) -> clap::ArgMatches {
        command()
            .try_get_matches_from(args)
            .expect("API command parses")
    }

    fn processing_package_source() -> String {
        format!(
            r#"{{"id":"{PACKAGE_SOURCE_ID}","createdAt":"2026-01-01T00:00:00Z","attempt":1,"status":"processing"}}"#
        )
    }

    fn assessment_status() -> String {
        format!(
            r#"{{"id":"{ASSESSMENT_ID}","state":"completed","affectedPurl":"pkg:npm/example@1.0.0","target":"pkg:npm/example@1.1.0","sourceRepositoryUrl":null,"recommendation":"upgrade","findingCount":0,"exitStatus":0,"errorKind":null,"errorMessage":null,"retryable":false}}"#
        )
    }

    fn upgrade_input() -> serde_json::Value {
        serde_json::json!({
            "packageSource": {
                "ecosystem": "npm",
                "fileName": "package-lock.json",
                "contents": "{}"
            },
            "vulnerablePackage": {
                "name": "example",
                "ecosystem": "npm",
                "version": "1.0.0",
                "purl": "pkg:npm/example@1.0.0"
            },
            "cveLinkage": ["CVE-2026-0001"],
            "kevLinkage": {
                "cveIds": ["CVE-2026-0001"],
                "knownExploited": true,
                "references": ["https://www.cisa.gov/known-exploited-vulnerabilities-catalog"]
            },
            "candidateVersion": "1.1.0"
        })
    }

    fn upgrade_status() -> String {
        format!(
            r#"{{"id":"{UPGRADE_ASSESSMENT_ID}","status":"pending","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}"#,
        )
    }

    fn upgrade_result() -> String {
        serde_json::json!({
            "id": UPGRADE_ASSESSMENT_ID,
            "status": "completed",
            "input": upgrade_input(),
            "verdict": "recommended",
            "summary": "Night Vision identified a recommended upgrade candidate.",
            "findings": [],
            "evidence": [],
            "caveats": [],
            "candidateVersions": [
                {
                    "version": "1.1.0",
                    "isRequestedCandidate": true,
                    "upgradeDistance": "minor",
                    "verdict": "recommended"
                }
            ],
            "assessedAt": "2026-01-01T00:00:00Z"
        })
        .to_string()
    }

    #[test]
    fn direct_api_commands_parse() {
        for args in [
            vec!["api", "health"],
            vec!["api", "health", "diagnostics"],
            vec!["api", "package-sources", "submit", "package.json"],
            vec![
                "api",
                "package-sources",
                "get",
                "0198f30e-2bfa-7000-8000-000000000007",
            ],
            vec![
                "api",
                "assessments",
                "submit",
                "--affected-purl",
                "pkg:npm/a@1",
                "--target-purl",
                "pkg:npm/a@2",
            ],
            vec![
                "api",
                "assessments",
                "get",
                "0198f30e-2bfa-7000-8000-000000000007",
            ],
            vec![
                "api",
                "assessments",
                "evidence",
                "0198f30e-2bfa-7000-8000-000000000007",
                "--include-raw-hipcheck",
            ],
            vec!["api", "upgrade-assessments", "submit", "request.json"],
            vec![
                "api",
                "upgrade-assessments",
                "get",
                "0198f30e-2bfa-7000-8000-000000000007",
            ],
            vec![
                "api",
                "upgrade-assessments",
                "result",
                "0198f30e-2bfa-7000-8000-000000000007",
            ],
        ] {
            command()
                .try_get_matches_from(args)
                .expect("command parses");
        }
    }

    #[test]
    fn package_source_submit_dispatches_json_request() {
        let mut files = TempFiles::default();
        let package = files.write("package.json", r#"{"name":"example"}"#);
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/package-sources")
                .header("content-type", "application/json")
                .header("user-agent", "nvdb")
                .json_body_obj(&serde_json::json!({
                    "fileName": package.file_name().expect("fixture filename"),
                    "contents": "{\"name\":\"example\"}"
                }));
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{PACKAGE_SOURCE_ID}"}}"#));
        });
        let (config, _files) = test_config(&server.base_url(), None);

        run(
            &config,
            &api_matches(&["api", "package-sources", "submit", package.as_str()]),
        )
        .expect("package source submission succeeds");
        mock.assert();
    }

    #[test]
    fn health_commands_dispatch_public_and_authenticated_requests() {
        let server = MockServer::start();
        let health = server.mock(|when, then| {
            when.method(GET)
                .path("/health")
                .header("user-agent", "nvdb");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"status":"ok"}"#);
        });
        let diagnostics = server.mock(|when, then| {
            when.method(GET)
                .path("/health/diagnostics")
                .header("user-agent", "nvdb")
                .header("authorization", "Bearer test-operator-token");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"status":"ok","cveIngest":{"recordsAvailable":false,"freshness":"unknown","lastSuccessfulSyncAt":null,"latestSuccessfulCommit":null,"latestRun":null},"kevIngest":{"recordsAvailable":false,"freshness":"unknown","lastSuccessfulSyncAt":null,"latestRun":null}}"#);
        });
        let (config, _files) = test_config(&server.base_url(), Some("test-operator-token"));

        run(&config, &api_matches(&["api", "health"])).expect("public health succeeds");
        run(&config, &api_matches(&["api", "health", "diagnostics"]))
            .expect("diagnostics health succeeds");

        health.assert();
        diagnostics.assert();
    }

    #[test]
    fn package_source_get_dispatches_id_path() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/package-sources/{PACKAGE_SOURCE_ID}"))
                .header("user-agent", "nvdb");
            then.status(200)
                .header("content-type", "application/json")
                .body(processing_package_source());
        });
        let (config, _files) = test_config(&server.base_url(), None);

        run(
            &config,
            &api_matches(&["api", "package-sources", "get", PACKAGE_SOURCE_ID]),
        )
        .expect("package source lookup succeeds");
        mock.assert();
    }

    #[test]
    fn assessment_commands_dispatch_all_request_shapes() {
        let server = MockServer::start();
        let submit = server.mock(|when, then| {
            when.method(POST)
                .path("/assessments")
                .header("content-type", "application/json")
                .json_body_obj(&serde_json::json!({
                    "affectedPurl": "pkg:npm/example@1.0.0",
                    "targetPurl": "pkg:npm/example@1.1.0"
                }));
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{ASSESSMENT_ID}"}}"#));
        });
        let get = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/assessments/{ASSESSMENT_ID}"));
            then.status(200)
                .header("content-type", "application/json")
                .body(assessment_status());
        });
        let evidence = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/assessments/{ASSESSMENT_ID}/evidence"))
                .query_param("includeRawHipcheck", "true");
            then.status(200)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{ASSESSMENT_ID}","affectedPurl":null,"diagnostics":{{"sourceRepositoryUrl":null,"stdout":null,"stdoutTruncated":false,"stderr":null,"stderrTruncated":false,"exitStatus":null,"errorKind":null,"errorMessage":null,"retryable":null}},"checks":[],"findings":[],"rawHipcheck":null}}"#));
        });
        let (config, _files) = test_config(&server.base_url(), None);

        run(
            &config,
            &api_matches(&[
                "api",
                "assessments",
                "submit",
                "--affected-purl",
                "pkg:npm/example@1.0.0",
                "--target-purl",
                "pkg:npm/example@1.1.0",
            ]),
        )
        .expect("assessment submit succeeds");
        run(
            &config,
            &api_matches(&["api", "assessments", "get", ASSESSMENT_ID]),
        )
        .expect("assessment lookup succeeds");
        run(
            &config,
            &api_matches(&[
                "api",
                "assessments",
                "evidence",
                ASSESSMENT_ID,
                "--include-raw-hipcheck",
            ]),
        )
        .expect("assessment evidence succeeds");

        submit.assert();
        get.assert();
        evidence.assert();
    }

    #[test]
    fn upgrade_assessment_commands_deserialize_and_dispatch_requests() {
        let server = MockServer::start();
        let request_body = upgrade_input();
        let submit = server.mock(|when, then| {
            when.method(POST)
                .path("/upgrade-assessments")
                .header("content-type", "application/json")
                .json_body_obj(&request_body);
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"id":"{UPGRADE_ASSESSMENT_ID}","status":"pending","createdAt":"2026-01-01T00:00:00Z","statusUrl":"/upgrade-assessments/{UPGRADE_ASSESSMENT_ID}","resultUrl":"/upgrade-assessments/{UPGRADE_ASSESSMENT_ID}/result"}}"#
                ));
        });
        let get = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/upgrade-assessments/{UPGRADE_ASSESSMENT_ID}"));
            then.status(200)
                .header("content-type", "application/json")
                .body(upgrade_status());
        });
        let result = server.mock(|when, then| {
            when.method(GET).path(format!(
                "/upgrade-assessments/{UPGRADE_ASSESSMENT_ID}/result"
            ));
            then.status(200)
                .header("content-type", "application/json")
                .body(upgrade_result());
        });
        let (config, _config_files) = test_config(&server.base_url(), None);
        let mut files = TempFiles::default();
        let request = files.write("upgrade-request.json", request_body.to_string());

        run(
            &config,
            &api_matches(&["api", "upgrade-assessments", "submit", request.as_str()]),
        )
        .expect("upgrade assessment submit succeeds");
        run(
            &config,
            &api_matches(&["api", "upgrade-assessments", "get", UPGRADE_ASSESSMENT_ID]),
        )
        .expect("upgrade assessment lookup succeeds");
        run(
            &config,
            &api_matches(&[
                "api",
                "upgrade-assessments",
                "result",
                UPGRADE_ASSESSMENT_ID,
            ]),
        )
        .expect("upgrade assessment result lookup succeeds");

        submit.assert();
        get.assert();
        result.assert();
    }

    #[test]
    fn diagnostics_without_a_credential_fails_before_sending_a_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/health/diagnostics");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"status":"ok","cveIngest":{"recordsAvailable":false,"freshness":"unknown","lastSuccessfulSyncAt":null,"latestSuccessfulCommit":null,"latestRun":null},"kevIngest":{"recordsAvailable":false,"freshness":"unknown","lastSuccessfulSyncAt":null,"latestRun":null}}"#);
        });
        let (config, _files) = test_config(&server.base_url(), None);

        let error = run(&config, &api_matches(&["api", "health", "diagnostics"]))
            .expect_err("diagnostics without a configured token must fail");

        assert!(
            error
                .to_string()
                .contains("health diagnostics requires health-diagnostics-token-file")
        );
        mock.assert_calls(0);
    }

    #[test]
    fn invalid_upgrade_request_file_fails_before_sending_a_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/upgrade-assessments");
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{UPGRADE_ASSESSMENT_ID}"}}"#));
        });
        let (config, _config_files) = test_config(&server.base_url(), None);
        let mut files = TempFiles::default();
        let request = files.write("invalid-upgrade-request.json", "{");

        let error = run(
            &config,
            &api_matches(&["api", "upgrade-assessments", "submit", request.as_str()]),
        )
        .expect_err("malformed upgrade request must fail");

        assert!(
            error
                .to_string()
                .contains("failed to parse upgrade assessment request JSON")
        );
        mock.assert_calls(0);
    }

    #[test]
    fn invalid_package_source_files_fail_before_sending_a_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/package-sources");
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{PACKAGE_SOURCE_ID}"}}"#));
        });
        let (config, _config_files) = test_config(&server.base_url(), None);
        let mut files = TempFiles::default();
        let non_utf8 = files.write("non-utf8-package.json", [0xff]);
        let missing = Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .expect("temporary directory path is UTF-8")
            .join(format!(
                "night-vision-missing-package-{}.json",
                uuid::Uuid::now_v7()
            ));

        for path in [&missing, &non_utf8] {
            let error = run(
                &config,
                &api_matches(&["api", "package-sources", "submit", path.as_str()]),
            )
            .expect_err("invalid package source file must fail");
            assert!(
                error
                    .to_string()
                    .contains("failed to read package source file")
            );
        }
        mock.assert_calls(0);
    }

    #[test]
    fn non_success_api_response_identifies_the_endpoint() {
        let server = MockServer::start();
        let non_success = server.mock(|when, then| {
            when.method(GET).path("/health");
            then.status(503)
                .header("content-type", "application/json")
                .body(r#"{"request_id":"test-request","message":"temporarily unavailable"}"#);
        });
        let (config, _files) = test_config(&server.base_url(), None);

        let error = run(&config, &api_matches(&["api", "health"]))
            .expect_err("non-success health response must fail");

        assert!(error.to_string().contains("GET /health failed"));
        assert!(error.to_string().contains("temporarily unavailable"));
        non_success.assert();
    }

    #[test]
    fn malformed_success_api_response_identifies_the_endpoint() {
        let server = MockServer::start();
        let malformed = server.mock(|when, then| {
            when.method(GET).path("/health");
            then.status(200)
                .header("content-type", "application/json")
                .body("not JSON");
        });
        let (config, _files) = test_config(&server.base_url(), None);
        let error = run(&config, &api_matches(&["api", "health"]))
            .expect_err("malformed health response must fail");
        assert!(error.to_string().contains("GET /health failed"));
        malformed.assert();
    }

    #[test]
    fn package_source_body_reads_utf8_package_source_file() {
        let path = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .expect("temporary directory path is UTF-8")
            .join(format!(
                "night-vision-package-source-{}.json",
                uuid::Uuid::now_v7()
            ));
        fs::write(&path, r#"{"name":"example-package"}"#).expect("write fixture");

        let body = package_source_body(&path).expect("read package source");

        fs::remove_file(&path).expect("remove fixture");
        assert_eq!(body.file_name, path.file_name().expect("fixture filename"));
        assert_eq!(body.contents, r#"{"name":"example-package"}"#);
    }

    #[test]
    fn package_source_body_preserves_file_name_when_package_name_missing() {
        let path = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .expect("temporary directory path is UTF-8")
            .join(format!(
                "night-vision-anonymous-package-source-{}.json",
                uuid::Uuid::now_v7()
            ));
        fs::write(&path, r#"{"private":true}"#).expect("write fixture");

        let body = package_source_body(&path).expect("read package source");

        fs::remove_file(&path).expect("remove fixture");
        assert_eq!(body.file_name, path.file_name().expect("fixture filename"));
        assert_eq!(body.contents, r#"{"private":true}"#);
    }

    #[test]
    fn package_source_body_rejects_path_without_filename() {
        let error = package_source_body(&camino::Utf8PathBuf::from("/"))
            .expect_err("root is not a package source file");
        assert!(error.to_string().contains("must name a file"));
    }

    #[test]
    fn package_source_body_rejects_missing_and_non_utf8_files() {
        let directory = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .expect("temporary directory path is UTF-8");
        let missing = directory.join(format!(
            "night-vision-missing-{}.json",
            uuid::Uuid::now_v7()
        ));
        let error = package_source_body(&missing).expect_err("missing file is rejected");
        assert!(
            error
                .to_string()
                .contains("failed to read package source file")
        );

        let non_utf8 = directory.join(format!(
            "night-vision-non-utf8-{}.json",
            uuid::Uuid::now_v7()
        ));
        fs::write(&non_utf8, [0xff]).expect("write non-UTF-8 fixture");
        let error = package_source_body(&non_utf8).expect_err("non-UTF-8 file is rejected");
        fs::remove_file(&non_utf8).expect("remove fixture");
        assert!(
            error
                .to_string()
                .contains("failed to read package source file")
        );
    }
}
