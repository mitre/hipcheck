//! Public read models for the MVP assessment work queue.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
	DataStatus, PackageSourceExposure, UpgradeAssessmentCaveat, UpgradeAssessmentEvidence,
	UpgradeAssessmentFinding, UpgradeAssessmentUpgradeDistance, UpgradeAssessmentVerdict,
};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentWorkQueueQuery {
	#[serde(default)]
	pub view: AssessmentQueueView,
	pub cursor: Option<String>,
	pub limit: Option<u32>,
	pub search: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema, Clone, Copy, Default, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentQueueView {
	#[default]
	NeedsReview,
	Processing,
	Completed,
}

/// Identifies one CVE-linked reachable package version in a submitted source.
#[derive(Deserialize, Serialize, JsonSchema, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentExposureReference {
	pub source_id: Uuid,
	pub package_id: Uuid,
	pub cve_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct AssessmentExposurePathParams {
	pub source_id: Uuid,
	pub package_id: Uuid,
	pub cve_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentExposureDetailQuery {
	pub candidate_cursor: Option<usize>,
	pub candidate_limit: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentWorkQueue {
	pub view: AssessmentQueueView,
	pub items: Vec<AssessmentQueueItem>,
	pub next_cursor: Option<String>,
	/// Counts refer only to the returned page. An unavailable dataset or an
	/// incomplete source means the full exposure count is unknown.
	pub coverage: AssessmentCoverage,
	pub data_status: DataStatus,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCoverage {
	pub scope: String,
	pub exposure_count: u32,
	pub unassessed_count: u32,
	pub processing_source_count: u32,
	pub unavailable_source_count: u32,
	pub complete: bool,
	pub message: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentQueueItem {
	/// Stable source/package/CVE key, or source key while resolution is incomplete.
	pub id: String,
	pub source: AssessmentSourceIdentity,
	pub exposure: Option<PackageSourceExposure>,
	pub reachability_truncated: bool,
	pub state: AssessmentReadState,
	pub assessment_id: Option<Uuid>,
	pub candidate_state: AssessmentCandidateState,
	pub candidate: Option<AssessmentCandidate>,
	pub verdict: Option<UpgradeAssessmentVerdict>,
	pub updated_at: DateTime<Utc>,
	pub detail_url: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentSourceIdentity {
	pub id: Uuid,
	pub name: Option<String>,
	pub file_name: String,
	pub lifecycle: String,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentReadState {
	NotAssessed,
	Processing,
	Completed,
	Failed,
	Unavailable,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentCandidateState {
	Available,
	NoCandidate,
	Processing,
	Failed,
	Unavailable,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentEvidenceState {
	Available,
	Missing,
	Processing,
	Failed,
	Unavailable,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentSelectionState {
	Selected,
	Available,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AssessmentSemverCompatibility {
	Compatible,
	Incompatible,
	NoGuarantee,
	Unknown,
}

#[derive(Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCandidate {
	/// Canonical NPM package URL when the package name is known; otherwise an
	/// assessment-scoped version key.
	pub id: String,
	pub version: String,
	pub upgrade_distance: UpgradeAssessmentUpgradeDistance,
	/// SemVer's API guarantee; it does not prove application compatibility.
	pub semver_compatibility: AssessmentSemverCompatibility,
	pub selection_state: AssessmentSelectionState,
	pub verdict: Option<UpgradeAssessmentVerdict>,
	pub major_upgrade_caution: Option<String>,
	pub evidence_state: AssessmentEvidenceState,
	pub findings: Vec<UpgradeAssessmentFinding>,
	pub findings_truncated: bool,
	pub evidence: Vec<UpgradeAssessmentEvidence>,
	pub evidence_truncated: bool,
	pub caveats: Vec<UpgradeAssessmentCaveat>,
	pub caveats_truncated: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentExposureDetail {
	pub source: AssessmentSourceIdentity,
	pub exposure: Option<PackageSourceExposure>,
	pub reachability_truncated: bool,
	pub state: AssessmentReadState,
	pub assessment_id: Option<Uuid>,
	pub candidate_state: AssessmentCandidateState,
	pub candidates: Vec<AssessmentCandidate>,
	pub next_candidate_cursor: Option<usize>,
	pub selected_candidate_id: Option<String>,
	pub verdict: Option<UpgradeAssessmentVerdict>,
	pub summary: Option<String>,
	pub caveats: Vec<UpgradeAssessmentCaveat>,
	pub caveats_truncated: bool,
	pub failure_message: Option<String>,
	pub data_status: DataStatus,
}
