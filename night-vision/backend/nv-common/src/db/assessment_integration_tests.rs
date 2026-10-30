//! Integration tests for the upgrade-assessment evidence schema: create,
//! update, lookup, and missing/deleted dependency behavior across
//! `upgrade_assessments` and its normalized child tables.

use crate::{
	config::Config,
	db::{
		connection,
		entities::{
			cve_list_records, package_versions, packages, upgrade_assessment_candidates,
			upgrade_assessment_caveats, upgrade_assessment_evidence_sources,
			upgrade_assessment_finding_evidence, upgrade_assessment_findings,
			upgrade_assessment_verdict_evidence, upgrade_assessment_verdicts, upgrade_assessments,
		},
	},
	test_util::with_integration_test_db_lock,
};
use camino::{Utf8Path, Utf8PathBuf};
use sea_orm::{
	ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection,
	EntityTrait as _, IntoActiveModel as _, QueryFilter as _,
};
use secrecy::ExposeSecret as _;
use serde_json::json;
use url::Url;

const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";
const TEST_ASSESSMENT_ID_PREFIX: &str = "upgrade-assessment-test-";
const TEST_PACKAGE_NAME_PREFIX: &str = "upgrade-assessment-test-pkg-";
const TEST_CVE_ID_PREFIX: &str = "CVE-2026-90002";
const TEST_CVE_ID_ROUND_TRIP: &str = "CVE-2026-9000201";
const TEST_CVE_ID_UPDATE_VERDICT: &str = "CVE-2026-9000202";
const TEST_CVE_ID_UPDATE_FINDING: &str = "CVE-2026-9000203";
const TEST_CVE_ID_SETNULL_PACKAGE_VERSION: &str = "CVE-2026-9000204";
const TEST_CVE_ID_SETNULL_CVE_RECORD: &str = "CVE-2026-9000205";
const TEST_CVE_ID_CASCADE: &str = "CVE-2026-9000206";

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn upgrade_assessment_full_tree_round_trips_through_postgres() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}1");
			let package_name = test_package_name(&assessment_id);

			let package_id = insert_package(&db, &package_name).await;
			let candidate_version_id =
				insert_package_version(&db, package_id, &package_name, "1.2.7").await;
			insert_cve_record(&db, TEST_CVE_ID_ROUND_TRIP).await;

			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_ROUND_TRIP,
				Some("1.2.7"),
			)
			.await;

			let candidate = insert_candidate(
				&db,
				&assessment_id,
				Some(candidate_version_id),
				"1.2.7",
				"patch",
			)
			.await;

			let verdict = insert_verdict(
				&db,
				candidate.id,
				"recommended",
				"fixes the KEV-linked CVE with no new risk signals",
				"high",
			)
			.await;

			let finding = insert_finding(
				&db,
				&assessment_id,
				Some(candidate.id),
				"night-vision",
				"vulnerability-status",
				"context",
				None,
				"candidate removes the KEV-listed exposure",
			)
			.await;

			let evidence = insert_evidence_source(
				&db,
				&assessment_id,
				"kev",
				Some(TEST_CVE_ID_ROUND_TRIP),
				Some("cisa-kev-catalog"),
				Some("KEV entry for the triggering CVE"),
				Some(json!({"cveID": TEST_CVE_ID_ROUND_TRIP})),
			)
			.await;

			upgrade_assessment_finding_evidence::Entity::insert(
				upgrade_assessment_finding_evidence::ActiveModel {
					finding_id: Set(finding.id),
					evidence_source_id: Set(evidence.id),
				},
			)
			.exec(&db)
			.await
			.expect("finding/evidence join row should insert");

			upgrade_assessment_verdict_evidence::Entity::insert(
				upgrade_assessment_verdict_evidence::ActiveModel {
					verdict_id: Set(verdict.id),
					evidence_source_id: Set(evidence.id),
				},
			)
			.exec(&db)
			.await
			.expect("verdict/evidence join row should insert");

			upgrade_assessment_caveats::Entity::insert(upgrade_assessment_caveats::ActiveModel {
				upgrade_assessment_id: Set(assessment_id.clone()),
				candidate_id: Set(Some(candidate.id)),
				caveat: Set("Night Vision cannot prove runtime compatibility".to_owned()),
				created_at: Default::default(),
				..Default::default()
			})
			.exec(&db)
			.await
			.expect("caveat should insert");

			let stored_candidates = upgrade_assessment_candidates::Entity::find()
				.filter(
					upgrade_assessment_candidates::Column::UpgradeAssessmentId
						.eq(assessment_id.clone()),
				)
				.all(&db)
				.await
				.expect("candidate lookup should succeed");
			assert_eq!(stored_candidates.len(), 1);
			assert_eq!(stored_candidates[0].candidate_version, "1.2.7");

			let stored_verdict = upgrade_assessment_verdicts::Entity::find()
				.filter(upgrade_assessment_verdicts::Column::CandidateId.eq(candidate.id))
				.one(&db)
				.await
				.expect("verdict lookup should succeed")
				.expect("verdict should be stored");
			assert_eq!(stored_verdict.verdict, "recommended");

			let stored_verdict_join = upgrade_assessment_verdict_evidence::Entity::find()
				.filter(
					upgrade_assessment_verdict_evidence::Column::VerdictId.eq(stored_verdict.id),
				)
				.all(&db)
				.await
				.expect("verdict/evidence join lookup should succeed");
			assert_eq!(stored_verdict_join.len(), 1);
			assert_eq!(stored_verdict_join[0].evidence_source_id, evidence.id);

			let stored_findings = upgrade_assessment_findings::Entity::find()
				.filter(
					upgrade_assessment_findings::Column::UpgradeAssessmentId
						.eq(assessment_id.clone()),
				)
				.all(&db)
				.await
				.expect("finding lookup should succeed");
			assert_eq!(stored_findings.len(), 1);

			let stored_evidence = upgrade_assessment_evidence_sources::Entity::find()
				.filter(
					upgrade_assessment_evidence_sources::Column::UpgradeAssessmentId
						.eq(assessment_id.clone()),
				)
				.all(&db)
				.await
				.expect("evidence lookup should succeed");
			assert_eq!(stored_evidence.len(), 1);
			assert_eq!(
				stored_evidence[0].raw_payload,
				Some(json!({"cveID": TEST_CVE_ID_ROUND_TRIP}))
			);

			let stored_join = upgrade_assessment_finding_evidence::Entity::find()
				.filter(upgrade_assessment_finding_evidence::Column::FindingId.eq(finding.id))
				.all(&db)
				.await
				.expect("finding/evidence join lookup should succeed");
			assert_eq!(stored_join.len(), 1);
			assert_eq!(stored_join[0].evidence_source_id, evidence.id);

			let stored_caveats = upgrade_assessment_caveats::Entity::find()
				.filter(
					upgrade_assessment_caveats::Column::UpgradeAssessmentId
						.eq(assessment_id.clone()),
				)
				.all(&db)
				.await
				.expect("caveat lookup should succeed");
			assert_eq!(stored_caveats.len(), 1);

			clear_assessment_test_data(&db).await;
		});
	});
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn upgrade_assessment_verdict_can_be_updated_after_creation() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}update-verdict");
			let package_name = test_package_name(&assessment_id);
			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_UPDATE_VERDICT,
				None,
			)
			.await;

			let candidate = insert_candidate(&db, &assessment_id, None, "1.2.7", "patch").await;

			let verdict = insert_verdict(
				&db,
				candidate.id,
				"unknown",
				"evidence not yet available",
				"insufficient",
			)
			.await;

			let mut updated = verdict.into_active_model();
			updated.verdict = Set("recommended".to_owned());
			updated.rationale = Set("re-scored once evidence arrived".to_owned());
			updated.evidence_quality = Set("high".to_owned());
			let updated = updated
				.update(&db)
				.await
				.expect("verdict should update in place");
			assert_eq!(updated.verdict, "recommended");
			assert_eq!(updated.evidence_quality, "high");

			let reloaded = upgrade_assessment_verdicts::Entity::find_by_id(updated.id)
				.one(&db)
				.await
				.expect("verdict lookup should succeed")
				.expect("updated verdict should still exist");
			assert_eq!(reloaded.verdict, "recommended");
			assert_eq!(reloaded.rationale, "re-scored once evidence arrived");
			assert_eq!(reloaded.evidence_quality, "high");

			clear_assessment_test_data(&db).await;
		});
	});
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn upgrade_assessment_finding_can_be_updated_after_creation() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}update-finding");
			let package_name = test_package_name(&assessment_id);
			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_UPDATE_FINDING,
				None,
			)
			.await;

			let candidate = insert_candidate(&db, &assessment_id, None, "1.2.7", "patch").await;

			let finding = insert_finding(
				&db,
				&assessment_id,
				Some(candidate.id),
				"night-vision",
				"vulnerability-status",
				"context",
				None,
				"candidate removes the KEV-listed exposure",
			)
			.await;

			let mut updated = finding.into_active_model();
			updated.effect = Set("risk".to_owned());
			updated.severity = Set(Some("low".to_owned()));
			updated.summary =
				Set("candidate introduces a low-severity compatibility risk".to_owned());
			let updated = updated
				.update(&db)
				.await
				.expect("finding should update in place");
			assert_eq!(updated.effect, "risk");
			assert_eq!(updated.severity.as_deref(), Some("low"));

			let reloaded = upgrade_assessment_findings::Entity::find_by_id(updated.id)
				.one(&db)
				.await
				.expect("finding lookup should succeed")
				.expect("updated finding should still exist");
			assert_eq!(reloaded.effect, "risk");
			assert_eq!(reloaded.severity.as_deref(), Some("low"));
			assert_eq!(
				reloaded.summary,
				"candidate introduces a low-severity compatibility risk"
			);

			clear_assessment_test_data(&db).await;
		});
	});
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn deleting_package_version_sets_candidate_reference_null() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}setnull");
			let package_name = test_package_name(&assessment_id);

			let package_id = insert_package(&db, &package_name).await;
			let candidate_version_id =
				insert_package_version(&db, package_id, &package_name, "1.2.7").await;

			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_SETNULL_PACKAGE_VERSION,
				None,
			)
			.await;

			let candidate = insert_candidate(
				&db,
				&assessment_id,
				Some(candidate_version_id),
				"1.2.7",
				"patch",
			)
			.await;

			package_versions::Entity::delete_by_id(candidate_version_id)
				.exec(&db)
				.await
				.expect("deleting the package_version row should succeed");

			let reloaded = upgrade_assessment_candidates::Entity::find_by_id(candidate.id)
				.one(&db)
				.await
				.expect("candidate lookup should succeed")
				.expect("candidate should still exist");
			assert_eq!(reloaded.candidate_package_version_id, None);
			assert_eq!(reloaded.candidate_version, "1.2.7");

			clear_assessment_test_data(&db).await;
		});
	});
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn deleting_cve_list_record_sets_evidence_reference_null() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}cve-setnull");
			let package_name = test_package_name(&assessment_id);

			insert_cve_record(&db, TEST_CVE_ID_SETNULL_CVE_RECORD).await;

			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_SETNULL_CVE_RECORD,
				None,
			)
			.await;

			let evidence = insert_evidence_source(
				&db,
				&assessment_id,
				"cve",
				Some(TEST_CVE_ID_SETNULL_CVE_RECORD),
				None,
				None,
				None,
			)
			.await;

			cve_list_records::Entity::delete_by_id(TEST_CVE_ID_SETNULL_CVE_RECORD.to_owned())
				.exec(&db)
				.await
				.expect("deleting the CVE record should succeed");

			let reloaded = upgrade_assessment_evidence_sources::Entity::find_by_id(evidence.id)
				.one(&db)
				.await
				.expect("evidence lookup should succeed")
				.expect("evidence should still exist");
			assert_eq!(reloaded.cve_id, None);

			clear_assessment_test_data(&db).await;
		});
	});
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn deleting_upgrade_assessment_cascades_to_children() {
	with_integration_test_db_lock(|| {
		run_async(async {
			let db = connect_to_integration_database().await;
			clear_assessment_test_data(&db).await;

			let assessment_id = format!("{TEST_ASSESSMENT_ID_PREFIX}cascade");
			let package_name = test_package_name(&assessment_id);
			insert_upgrade_assessment(
				&db,
				&assessment_id,
				&package_name,
				TEST_CVE_ID_CASCADE,
				None,
			)
			.await;

			let candidate = insert_candidate(&db, &assessment_id, None, "1.2.7", "patch").await;

			let verdict =
				insert_verdict(&db, candidate.id, "recommended", "test rationale", "high").await;

			let finding = insert_finding(
				&db,
				&assessment_id,
				Some(candidate.id),
				"night-vision",
				"vulnerability-status",
				"context",
				None,
				"test finding",
			)
			.await;

			let evidence = insert_evidence_source(
				&db,
				&assessment_id,
				"package-source",
				None,
				None,
				None,
				None,
			)
			.await;

			upgrade_assessment_finding_evidence::Entity::insert(
				upgrade_assessment_finding_evidence::ActiveModel {
					finding_id: Set(finding.id),
					evidence_source_id: Set(evidence.id),
				},
			)
			.exec(&db)
			.await
			.expect("join row should insert");

			upgrade_assessment_verdict_evidence::Entity::insert(
				upgrade_assessment_verdict_evidence::ActiveModel {
					verdict_id: Set(verdict.id),
					evidence_source_id: Set(evidence.id),
				},
			)
			.exec(&db)
			.await
			.expect("verdict/evidence join row should insert");

			upgrade_assessment_caveats::Entity::insert(upgrade_assessment_caveats::ActiveModel {
				upgrade_assessment_id: Set(assessment_id.clone()),
				candidate_id: Set(Some(candidate.id)),
				caveat: Set("test caveat".to_owned()),
				created_at: Default::default(),
				..Default::default()
			})
			.exec(&db)
			.await
			.expect("caveat should insert");

			upgrade_assessments::Entity::delete_by_id(assessment_id.clone())
				.exec(&db)
				.await
				.expect("deleting the upgrade assessment should succeed");

			assert!(
				upgrade_assessment_candidates::Entity::find_by_id(candidate.id)
					.one(&db)
					.await
					.expect("candidate lookup should succeed")
					.is_none()
			);
			assert!(
				upgrade_assessment_findings::Entity::find_by_id(finding.id)
					.one(&db)
					.await
					.expect("finding lookup should succeed")
					.is_none()
			);
			assert!(
				upgrade_assessment_evidence_sources::Entity::find_by_id(evidence.id)
					.one(&db)
					.await
					.expect("evidence lookup should succeed")
					.is_none()
			);
			assert!(
				upgrade_assessment_finding_evidence::Entity::find_by_id((finding.id, evidence.id))
					.one(&db)
					.await
					.expect("join lookup should succeed")
					.is_none()
			);
			assert!(
				upgrade_assessment_verdict_evidence::Entity::find_by_id((verdict.id, evidence.id))
					.one(&db)
					.await
					.expect("verdict/evidence join lookup should succeed")
					.is_none()
			);
			assert!(
				upgrade_assessment_caveats::Entity::find()
					.filter(
						upgrade_assessment_caveats::Column::UpgradeAssessmentId
							.eq(assessment_id.clone())
					)
					.all(&db)
					.await
					.expect("caveat lookup should succeed")
					.is_empty()
			);

			clear_assessment_test_data(&db).await;
		});
	});
}

async fn insert_upgrade_assessment(
	db: &DatabaseConnection,
	assessment_id: &str,
	package_name: &str,
	cve_id: &str,
	candidate_version: Option<&str>,
) {
	let assessment = upgrade_assessments::ActiveModel {
		id: Set(assessment_id.to_owned()),
		package_name: Set(package_name.to_owned()),
		current_version: Set("1.2.3".to_owned()),
		trigger_kind: Set("cve".to_owned()),
		trigger_reference: Set(cve_id.to_owned()),
		candidate_version: Set(candidate_version.map(str::to_owned)),
		status: Set("processing".to_owned()),
		created_at: Default::default(),
		finished_at: Set(None),
		report: Set(None),
		error: Set(None),
	};
	upgrade_assessments::Entity::insert(assessment)
		.exec(db)
		.await
		.expect("upgrade assessment should insert");
}

async fn insert_candidate(
	db: &DatabaseConnection,
	assessment_id: &str,
	candidate_package_version_id: Option<i32>,
	candidate_version: &str,
	upgrade_distance: &str,
) -> upgrade_assessment_candidates::Model {
	let candidate = upgrade_assessment_candidates::ActiveModel {
		upgrade_assessment_id: Set(assessment_id.to_owned()),
		candidate_package_version_id: Set(candidate_package_version_id),
		candidate_version: Set(candidate_version.to_owned()),
		upgrade_distance: Set(upgrade_distance.to_owned()),
		created_at: Default::default(),
		..Default::default()
	};
	upgrade_assessment_candidates::Entity::insert(candidate)
		.exec(db)
		.await
		.expect("candidate should insert");

	upgrade_assessment_candidates::Entity::find()
		.filter(upgrade_assessment_candidates::Column::UpgradeAssessmentId.eq(assessment_id))
		.filter(upgrade_assessment_candidates::Column::CandidateVersion.eq(candidate_version))
		.one(db)
		.await
		.expect("candidate lookup should succeed")
		.expect("candidate should exist after insert")
}

async fn insert_verdict(
	db: &DatabaseConnection,
	candidate_id: i32,
	verdict: &str,
	rationale: &str,
	evidence_quality: &str,
) -> upgrade_assessment_verdicts::Model {
	let model = upgrade_assessment_verdicts::ActiveModel {
		candidate_id: Set(candidate_id),
		verdict: Set(verdict.to_owned()),
		rationale: Set(rationale.to_owned()),
		evidence_quality: Set(evidence_quality.to_owned()),
		computed_at: Default::default(),
		..Default::default()
	};
	upgrade_assessment_verdicts::Entity::insert(model)
		.exec(db)
		.await
		.expect("verdict should insert");

	upgrade_assessment_verdicts::Entity::find()
		.filter(upgrade_assessment_verdicts::Column::CandidateId.eq(candidate_id))
		.one(db)
		.await
		.expect("verdict lookup should succeed")
		.expect("verdict should exist after insert")
}

#[expect(
	clippy::too_many_arguments,
	reason = "integration-test helpers keep the stored finding shape flat"
)]
async fn insert_finding(
	db: &DatabaseConnection,
	assessment_id: &str,
	candidate_id: Option<i32>,
	source: &str,
	detection_kind: &str,
	effect: &str,
	severity: Option<&str>,
	summary: &str,
) -> upgrade_assessment_findings::Model {
	let finding = upgrade_assessment_findings::ActiveModel {
		upgrade_assessment_id: Set(assessment_id.to_owned()),
		candidate_id: Set(candidate_id),
		source: Set(source.to_owned()),
		detection_kind: Set(detection_kind.to_owned()),
		effect: Set(effect.to_owned()),
		severity: Set(severity.map(str::to_owned)),
		summary: Set(summary.to_owned()),
		created_at: Default::default(),
		..Default::default()
	};
	upgrade_assessment_findings::Entity::insert(finding)
		.exec(db)
		.await
		.expect("finding should insert");

	upgrade_assessment_findings::Entity::find()
		.filter(upgrade_assessment_findings::Column::UpgradeAssessmentId.eq(assessment_id))
		.filter(upgrade_assessment_findings::Column::CandidateId.eq(candidate_id))
		.filter(upgrade_assessment_findings::Column::Source.eq(source))
		.filter(upgrade_assessment_findings::Column::DetectionKind.eq(detection_kind))
		.filter(upgrade_assessment_findings::Column::Effect.eq(effect))
		.filter(upgrade_assessment_findings::Column::Summary.eq(summary))
		.one(db)
		.await
		.expect("finding lookup should succeed")
		.expect("finding should exist after insert")
}

async fn insert_evidence_source(
	db: &DatabaseConnection,
	assessment_id: &str,
	kind: &str,
	cve_id: Option<&str>,
	reference: Option<&str>,
	summary: Option<&str>,
	raw_payload: Option<serde_json::Value>,
) -> upgrade_assessment_evidence_sources::Model {
	let evidence = upgrade_assessment_evidence_sources::ActiveModel {
		upgrade_assessment_id: Set(assessment_id.to_owned()),
		kind: Set(kind.to_owned()),
		cve_id: Set(cve_id.map(str::to_owned)),
		reference: Set(reference.map(str::to_owned)),
		summary: Set(summary.map(str::to_owned)),
		raw_payload: Set(raw_payload.clone()),
		captured_at: Default::default(),
		..Default::default()
	};
	upgrade_assessment_evidence_sources::Entity::insert(evidence)
		.exec(db)
		.await
		.expect("evidence should insert");

	upgrade_assessment_evidence_sources::Entity::find()
		.filter(upgrade_assessment_evidence_sources::Column::UpgradeAssessmentId.eq(assessment_id))
		.filter(upgrade_assessment_evidence_sources::Column::Kind.eq(kind))
		.filter(upgrade_assessment_evidence_sources::Column::CveId.eq(cve_id))
		.filter(upgrade_assessment_evidence_sources::Column::Reference.eq(reference))
		.one(db)
		.await
		.expect("evidence lookup should succeed")
		.expect("evidence should exist after insert")
}

fn test_package_name(assessment_id: &str) -> String {
	format!("{TEST_PACKAGE_NAME_PREFIX}{assessment_id}")
}

async fn insert_package(db: &DatabaseConnection, package_name: &str) -> i32 {
	let package = packages::ActiveModel {
		name: Set(package_name.to_owned()),
		package_host: Set("npm".to_owned()),
		..Default::default()
	};
	packages::Entity::insert(package)
		.exec_with_returning(db)
		.await
		.expect("package should insert")
		.id
}

async fn insert_package_version(
	db: &DatabaseConnection,
	package_id: i32,
	package_name: &str,
	version: &str,
) -> i32 {
	let package_version = package_versions::ActiveModel {
		package_id: Set(package_id),
		version: Set(version.to_owned()),
		package_url: Set(format!("pkg:npm/{package_name}@{version}")),
		source_repository: Set(None),
		source_repository_tag: Set(None),
		..Default::default()
	};
	package_versions::Entity::insert(package_version)
		.exec_with_returning(db)
		.await
		.expect("package version should insert")
		.id
}

async fn insert_cve_record(db: &DatabaseConnection, cve_id: &str) {
	let record = cve_list_records::ActiveModel {
		cve_id: Set(cve_id.to_owned()),
		record_format_version: Set("5.2".to_owned()),
		record: Set(json!({
			"dataType": "CVE_RECORD",
			"dataVersion": "5.2",
			"cveMetadata": {
				"cveId": cve_id
			}
		})),
		deleted: Set(false),
		first_seen_at: Default::default(),
		last_seen_at: Default::default(),
		updated_at: Default::default(),
	};
	cve_list_records::Entity::insert(record)
		.exec(db)
		.await
		.expect("CVE record should insert");
}

async fn clear_assessment_test_data(db: &DatabaseConnection) {
	upgrade_assessments::Entity::delete_many()
		.filter(upgrade_assessments::Column::Id.starts_with(TEST_ASSESSMENT_ID_PREFIX))
		.exec(db)
		.await
		.expect("upgrade assessments should clear");

	let test_package_ids = packages::Entity::find()
		.filter(packages::Column::Name.starts_with(TEST_PACKAGE_NAME_PREFIX))
		.all(db)
		.await
		.expect("test package lookup should succeed")
		.into_iter()
		.map(|package| package.id)
		.collect::<Vec<_>>();

	if !test_package_ids.is_empty() {
		package_versions::Entity::delete_many()
			.filter(package_versions::Column::PackageId.is_in(test_package_ids))
			.exec(db)
			.await
			.expect("package versions for test packages should clear");
	}

	package_versions::Entity::delete_many()
		.filter(
			package_versions::Column::PackageUrl
				.starts_with(format!("pkg:npm/{TEST_PACKAGE_NAME_PREFIX}")),
		)
		.exec(db)
		.await
		.expect("package versions should clear");
	packages::Entity::delete_many()
		.filter(packages::Column::Name.starts_with(TEST_PACKAGE_NAME_PREFIX))
		.exec(db)
		.await
		.expect("packages should clear");
	cve_list_records::Entity::delete_many()
		.filter(cve_list_records::Column::CveId.starts_with(TEST_CVE_ID_PREFIX))
		.exec(db)
		.await
		.expect("CVE records should clear");
}

async fn connect_to_integration_database() -> DatabaseConnection {
	let config_path = integration_config_path();
	let config = Config::parse(&config_path).expect("integration-test config should parse");
	let database_url = config.database_connection().expose_secret();
	assert_disposable_database_url(database_url);

	connection(&config).await.unwrap_or_else(|error| {
		panic!(
			"{}",
			integration_database_connection_error(&config_path, database_url, &error)
		)
	})
}

fn integration_config_path() -> Utf8PathBuf {
	std::env::var(CONFIG_PATH_ENV).map_or_else(
		|_| {
			let mut path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
			path.push(DEFAULT_CONFIG_PATH);
			path
		},
		Utf8PathBuf::from,
	)
}

fn assert_disposable_database_url(database_url: &str) {
	let url = Url::parse(database_url).expect("database URL should parse");
	assert!(
		matches!(url.scheme(), "postgres" | "postgresql"),
		"integration test database connection must use a Postgres URL"
	);

	let database = url.path().trim_start_matches('/');
	assert!(
		database.contains("test") || database.contains("integration"),
		"integration test database name must contain 'test' or 'integration'; got {database:?}"
	);
}

fn integration_database_connection_error(
	config_path: &Utf8Path,
	database_url: &str,
	error: &dyn std::error::Error,
) -> String {
	let database = Url::parse(database_url)
		.ok()
		.map(|url| url.path().trim_start_matches('/').to_owned())
		.filter(|database| !database.is_empty())
		.unwrap_or_else(|| "<unknown>".to_owned());

	format!(
		"Postgres integration database should connect and migrate.\n\
         \n\
         Config path: {config_path}\n\
         Database: {database}\n\
         \n\
         If the database does not exist, create it first:\n\
         \n\
             createdb {database}\n\
         \n\
         To use another disposable Postgres database, set {CONFIG_PATH_ENV} to a \
         server config file with a database-connection value whose database name \
         contains 'test' or 'integration'.\n\
         \n\
         Original error: {error:#}"
	)
}

fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime should build")
		.block_on(future)
}
