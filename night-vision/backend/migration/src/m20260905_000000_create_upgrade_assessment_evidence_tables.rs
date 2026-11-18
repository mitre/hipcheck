use crate::m20260708_150958_create_package_version_table::PackageVersion;
use sea_orm_migration::{
	prelude::*,
	schema::{integer, integer_null, pk_auto, string, string_null, text, text_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		// Candidate upgrade versions considered within one `upgrade_assessments` row.
		// `upgrade_assessments.candidate_version` remains the single-candidate summary
		// field the API already serves; this table lets an assessment record several
		// candidates, each with its own verdict, once the generation pipeline populates
		// more than one.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentCandidates::Table)
					.if_not_exists()
					.col(pk_auto(UpgradeAssessmentCandidates::Id))
					.col(string(UpgradeAssessmentCandidates::UpgradeAssessmentId))
					.col(integer_null(
						UpgradeAssessmentCandidates::CandidatePackageVersionId,
					))
					.col(string(UpgradeAssessmentCandidates::CandidateVersion))
					.col(string(UpgradeAssessmentCandidates::UpgradeDistance))
					.col(
						ColumnDef::new(UpgradeAssessmentCandidates::CreatedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentCandidates::Table,
								UpgradeAssessmentCandidates::UpgradeAssessmentId,
							)
							.to(Alias::new("upgrade_assessments"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentCandidates::Table,
								UpgradeAssessmentCandidates::CandidatePackageVersionId,
							)
							.to(PackageVersion::Table, PackageVersion::Id)
							.on_delete(ForeignKeyAction::SetNull),
					)
					.index(
						Index::create()
							.unique()
							.col(UpgradeAssessmentCandidates::UpgradeAssessmentId)
							.col(UpgradeAssessmentCandidates::CandidateVersion),
					)
					.check(Expr::cust(
						"upgrade_distance IN ('patch', 'minor', 'major')",
					))
					.to_owned(),
			)
			.await?;

		// Verdict for a single candidate. 1:1 with `upgrade_assessment_candidates`.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentVerdicts::Table)
					.if_not_exists()
					.col(pk_auto(UpgradeAssessmentVerdicts::Id))
					.col(integer(UpgradeAssessmentVerdicts::CandidateId))
					.col(string(UpgradeAssessmentVerdicts::Verdict))
					.col(text(UpgradeAssessmentVerdicts::Rationale))
					.col(string(UpgradeAssessmentVerdicts::EvidenceQuality))
					.col(
						ColumnDef::new(UpgradeAssessmentVerdicts::ComputedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentVerdicts::Table,
								UpgradeAssessmentVerdicts::CandidateId,
							)
							.to(
								UpgradeAssessmentCandidates::Table,
								UpgradeAssessmentCandidates::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.index(
						Index::create()
							.unique()
							.col(UpgradeAssessmentVerdicts::CandidateId),
					)
					.check(Expr::cust(
						"verdict IN ('recommended', 'caution', 'avoid', 'unknown')",
					))
					.check(Expr::cust(
						"evidence_quality IN ('high', 'limited', 'insufficient')",
					))
					.to_owned(),
			)
			.await?;

		// Findings normalized from both Night Vision's own checks and Hipcheck.
		// `candidate_id` is nullable: some findings (e.g. "current version is
		// KEV-listed") describe the whole assessment rather than one candidate.
		// `detection_kind`/`effect`/`severity` are left unconstrained, matching
		// this workspace's existing `hipcheck_checks`/`hipcheck_findings` tables,
		// since Hipcheck's plugin and effect vocabulary is expected to grow.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentFindings::Table)
					.if_not_exists()
					.col(pk_auto(UpgradeAssessmentFindings::Id))
					.col(string(UpgradeAssessmentFindings::UpgradeAssessmentId))
					.col(integer_null(UpgradeAssessmentFindings::CandidateId))
					.col(string(UpgradeAssessmentFindings::Source))
					.col(string(UpgradeAssessmentFindings::DetectionKind))
					.col(string(UpgradeAssessmentFindings::Effect))
					.col(string_null(UpgradeAssessmentFindings::Severity))
					.col(text(UpgradeAssessmentFindings::Summary))
					.col(
						ColumnDef::new(UpgradeAssessmentFindings::CreatedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentFindings::Table,
								UpgradeAssessmentFindings::UpgradeAssessmentId,
							)
							.to(Alias::new("upgrade_assessments"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentFindings::Table,
								UpgradeAssessmentFindings::CandidateId,
							)
							.to(
								UpgradeAssessmentCandidates::Table,
								UpgradeAssessmentCandidates::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.check(Expr::cust("source IN ('night-vision', 'hipcheck')"))
					.to_owned(),
			)
			.await?;

		// Normalized evidence (KEV/CVE/Hipcheck/etc.) referenced by findings and
		// verdicts through join tables.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentEvidenceSources::Table)
					.if_not_exists()
					.col(pk_auto(UpgradeAssessmentEvidenceSources::Id))
					.col(string(
						UpgradeAssessmentEvidenceSources::UpgradeAssessmentId,
					))
					.col(string(UpgradeAssessmentEvidenceSources::Kind))
					.col(string_null(UpgradeAssessmentEvidenceSources::CveId))
					.col(string_null(UpgradeAssessmentEvidenceSources::Reference))
					.col(text_null(UpgradeAssessmentEvidenceSources::Summary))
					.col(ColumnDef::new(UpgradeAssessmentEvidenceSources::RawPayload).json_binary())
					.col(
						ColumnDef::new(UpgradeAssessmentEvidenceSources::CapturedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentEvidenceSources::Table,
								UpgradeAssessmentEvidenceSources::UpgradeAssessmentId,
							)
							.to(Alias::new("upgrade_assessments"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentEvidenceSources::Table,
								UpgradeAssessmentEvidenceSources::CveId,
							)
							.to(Alias::new("cve_list_records"), Alias::new("cve_id"))
							.on_delete(ForeignKeyAction::SetNull),
					)
					.check(Expr::cust(
						"kind IN ('kev', 'cve', 'hipcheck', 'package-source', 'registry-metadata')",
					))
					.to_owned(),
			)
			.await?;

		// Join table: a finding may cite multiple evidence sources and an evidence
		// source may support multiple findings.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentFindingEvidence::Table)
					.if_not_exists()
					.col(integer(UpgradeAssessmentFindingEvidence::FindingId))
					.col(integer(UpgradeAssessmentFindingEvidence::EvidenceSourceId))
					.primary_key(
						Index::create()
							.col(UpgradeAssessmentFindingEvidence::FindingId)
							.col(UpgradeAssessmentFindingEvidence::EvidenceSourceId),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentFindingEvidence::Table,
								UpgradeAssessmentFindingEvidence::FindingId,
							)
							.to(
								UpgradeAssessmentFindings::Table,
								UpgradeAssessmentFindings::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentFindingEvidence::Table,
								UpgradeAssessmentFindingEvidence::EvidenceSourceId,
							)
							.to(
								UpgradeAssessmentEvidenceSources::Table,
								UpgradeAssessmentEvidenceSources::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.to_owned(),
			)
			.await?;

		// Join table: a verdict may cite multiple evidence sources and an evidence
		// source may support multiple verdicts.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentVerdictEvidence::Table)
					.if_not_exists()
					.col(integer(UpgradeAssessmentVerdictEvidence::VerdictId))
					.col(integer(UpgradeAssessmentVerdictEvidence::EvidenceSourceId))
					.primary_key(
						Index::create()
							.col(UpgradeAssessmentVerdictEvidence::VerdictId)
							.col(UpgradeAssessmentVerdictEvidence::EvidenceSourceId),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentVerdictEvidence::Table,
								UpgradeAssessmentVerdictEvidence::VerdictId,
							)
							.to(
								UpgradeAssessmentVerdicts::Table,
								UpgradeAssessmentVerdicts::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentVerdictEvidence::Table,
								UpgradeAssessmentVerdictEvidence::EvidenceSourceId,
							)
							.to(
								UpgradeAssessmentEvidenceSources::Table,
								UpgradeAssessmentEvidenceSources::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.to_owned(),
			)
			.await?;

		// Free-text caveats surfaced alongside an assessment or one of its candidates.
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessmentCaveats::Table)
					.if_not_exists()
					.col(pk_auto(UpgradeAssessmentCaveats::Id))
					.col(string(UpgradeAssessmentCaveats::UpgradeAssessmentId))
					.col(integer_null(UpgradeAssessmentCaveats::CandidateId))
					.col(text(UpgradeAssessmentCaveats::Caveat))
					.col(
						ColumnDef::new(UpgradeAssessmentCaveats::CreatedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentCaveats::Table,
								UpgradeAssessmentCaveats::UpgradeAssessmentId,
							)
							.to(Alias::new("upgrade_assessments"), Alias::new("id"))
							.on_delete(ForeignKeyAction::Cascade),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								UpgradeAssessmentCaveats::Table,
								UpgradeAssessmentCaveats::CandidateId,
							)
							.to(
								UpgradeAssessmentCandidates::Table,
								UpgradeAssessmentCandidates::Id,
							)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentCaveats::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentVerdictEvidence::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentFindingEvidence::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentEvidenceSources::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentFindings::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentVerdicts::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_table(
				Table::drop()
					.table(UpgradeAssessmentCandidates::Table)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
enum UpgradeAssessmentCandidates {
	Table,
	Id,
	UpgradeAssessmentId,
	CandidatePackageVersionId,
	CandidateVersion,
	UpgradeDistance,
	CreatedAt,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentVerdicts {
	Table,
	Id,
	CandidateId,
	Verdict,
	Rationale,
	EvidenceQuality,
	ComputedAt,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentFindings {
	Table,
	Id,
	UpgradeAssessmentId,
	CandidateId,
	Source,
	DetectionKind,
	Effect,
	Severity,
	Summary,
	CreatedAt,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentEvidenceSources {
	Table,
	Id,
	UpgradeAssessmentId,
	Kind,
	CveId,
	Reference,
	Summary,
	RawPayload,
	CapturedAt,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentFindingEvidence {
	Table,
	FindingId,
	EvidenceSourceId,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentVerdictEvidence {
	Table,
	VerdictId,
	EvidenceSourceId,
}

#[derive(DeriveIden)]
enum UpgradeAssessmentCaveats {
	Table,
	Id,
	UpgradeAssessmentId,
	CandidateId,
	Caveat,
	CreatedAt,
}
