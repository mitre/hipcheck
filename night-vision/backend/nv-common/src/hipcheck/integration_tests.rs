use super::{
	HipcheckReportContext, parse_hipcheck_report,
	storage::{HipcheckExecutionDiagnostics, load_hipcheck_run, store_hipcheck_run},
};
use crate::{config::Config, db::connection};
use camino::Utf8PathBuf;
use sea_orm::{ConnectionTrait as _, DatabaseConnection};

const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";
const REPORT: &str = include_str!("../../testdata/define-hipcheck/fixtures/native-315-mixed.json");
const PACKAGE_VERSION_ID: i32 = 920_001;

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn hipcheck_evidence_round_trips_through_postgres() {
	run_async(async {
		let db = integration_database().await;
		execute(&db, "DELETE FROM hipcheck_findings; DELETE FROM hipcheck_concerns; DELETE FROM hipcheck_checks; DELETE FROM hipcheck_runs; DELETE FROM package_version WHERE id = 920001; DELETE FROM packages WHERE id = 920001;").await;
		execute(&db, "INSERT INTO packages (id, name, package_host) VALUES (920001, 'example', 'npm'); INSERT INTO package_version (id, package_id, version, package_url, source_repository, source_repository_tag) VALUES (920001, 920001, '1.2.7', 'pkg:npm/example@1.2.7', 'https://github.com/example/name', NULL);").await;
		let report = parse_hipcheck_report(
			REPORT,
			&HipcheckReportContext {
				target_purl: "pkg:npm/name@1.2.7".to_owned(),
				source_repository_url: "https://github.com/example/name".to_owned(),
				policy_source: "/opt/night-vision/hipcheck/config/Hipcheck.kdl".to_owned(),
			},
		)
		.expect("fixture report");
		let run_id = store_hipcheck_run(
			&db,
			PACKAGE_VERSION_ID,
			REPORT,
			&report,
			&HipcheckExecutionDiagnostics {
				status: "completed".to_owned(),
				stdout: "report emitted".to_owned(),
				stderr: "plugin warning".to_owned(),
				exit_status: Some(0),
				error_kind: None,
				error_message: None,
				retryable: None,
			},
		)
		.await
		.expect("store evidence");
		let stored = load_hipcheck_run(&db, run_id)
			.await
			.expect("load evidence")
			.expect("stored run");
		assert_eq!(stored.run.raw_json.as_deref(), Some(REPORT));
		assert_eq!(
			stored.run.target_purl.as_deref(),
			Some("pkg:npm/name@1.2.7")
		);
		assert_eq!(
			stored.run.source_repository_url.as_deref(),
			Some("https://github.com/example/name")
		);
		assert_eq!(
			stored.run.policy_id.as_deref(),
			Some("night-vision-upgrade-assessment")
		);
		assert_eq!(stored.run.hipcheck_commit.as_deref(), Some("unknown"));
		assert_eq!(stored.checks.len(), 2);
		assert_eq!(stored.concerns.len(), 2);
		assert_eq!(stored.findings.len(), 4);
		assert_eq!(stored.findings[0].kind, "check-result");
		assert_eq!(stored.findings[1].kind, "missing-evidence");
		assert_eq!(stored.findings[2].kind, "check-error");
		assert_eq!(stored.findings[2].effect, "missing-check");
		assert_eq!(stored.findings[3].kind, "missing-evidence");
	});
}

async fn integration_database() -> DatabaseConnection {
	let config_path = std::env::var(CONFIG_PATH_ENV).map_or_else(
		|_| Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_CONFIG_PATH),
		Utf8PathBuf::from,
	);
	let config = Config::parse(&config_path).expect("integration configuration");
	connection(&config)
		.await
		.expect("disposable integration database should connect and migrate")
}
async fn execute(db: &DatabaseConnection, sql: &str) {
	db.execute_unprepared(sql).await.expect("test setup SQL");
}
fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime")
		.block_on(future)
}
