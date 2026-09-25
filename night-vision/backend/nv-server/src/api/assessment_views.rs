//! Exposure-backed assessment read models. No GET handler queues analysis.

use super::*;
use nv_common::db::entities::package_sources;
use nv_server_api::assessment_views::{
    AssessmentCandidate, AssessmentCandidateState, AssessmentCoverage, AssessmentEvidenceState,
    AssessmentExposureDetail, AssessmentExposureDetailQuery, AssessmentExposurePathParams,
    AssessmentExposureReference, AssessmentQueueItem, AssessmentQueueView, AssessmentReadState,
    AssessmentSelectionState, AssessmentSemverCompatibility, AssessmentSourceIdentity,
    AssessmentWorkQueue, AssessmentWorkQueueQuery,
};
use std::collections::HashMap;

const MAX_PAGE_SIZE: u32 = 50;
const MAX_SOURCES_PER_PAGE: usize = 20;
const MAX_REACHABILITY_PATHS: usize = 20;
const MAX_REACHABILITY_DEPTH: usize = 20;
const MAX_SUPPORTING_ITEMS: usize = 50;
const MAJOR_CAUTION: &str = "Night Vision cannot establish application compatibility for a major upgrade, so this candidate cannot receive a recommended verdict.";

pub(super) fn exposure_key(reference: &AssessmentExposureReference) -> String {
    format!(
        "{}:{}:{}",
        reference.source_id, reference.package_id, reference.cve_id
    )
}

fn exposure_reference(
    source_id: Uuid,
    exposure: &PackageSourceExposure,
) -> AssessmentExposureReference {
    AssessmentExposureReference {
        source_id,
        package_id: exposure.package.id,
        cve_id: exposure.cve_id.clone(),
    }
}

fn detail_url(reference: &AssessmentExposureReference) -> String {
    format!(
        "/assessment-exposures/{}/{}/{}",
        reference.source_id, reference.package_id, reference.cve_id
    )
}

fn bound_exposure(mut exposure: PackageSourceExposure) -> (PackageSourceExposure, bool) {
    let mut truncated = exposure.package.derivations.len() > MAX_REACHABILITY_PATHS;
    exposure
        .package
        .derivations
        .truncate(MAX_REACHABILITY_PATHS);
    for path in &mut exposure.package.derivations {
        if path.len() > MAX_REACHABILITY_DEPTH {
            truncated = true;
            let affected = path.last().cloned();
            path.truncate(MAX_REACHABILITY_DEPTH.saturating_sub(1));
            if let Some(affected) = affected {
                path.push(affected);
            }
        }
    }
    (exposure, truncated)
}

fn source_identity(source: &package_sources::Model) -> Result<AssessmentSourceIdentity, HttpError> {
    let name = serde_json::from_str::<serde_json::Value>(&source.file_contents)
        .ok()
        .and_then(|value| {
            value
                .get("name")
                .and_then(|name| name.as_str())
                .map(summary_text)
        });
    Ok(AssessmentSourceIdentity {
        id: Uuid::parse_str(&source.source_id).map_err(internal_error)?,
        name,
        file_name: source.file_name.clone(),
        lifecycle: source.resolution_status.clone(),
    })
}

fn source_status_item(
    source: &package_sources::Model,
    state: AssessmentReadState,
) -> Result<AssessmentQueueItem, HttpError> {
    let identity = source_identity(source)?;
    Ok(AssessmentQueueItem {
        id: format!("source:{}", identity.id),
        detail_url: format!("/package-sources/{}", identity.id),
        source: identity,
        exposure: None,
        reachability_truncated: false,
        state,
        assessment_id: None,
        candidate_state: match state {
            AssessmentReadState::Processing => AssessmentCandidateState::Processing,
            AssessmentReadState::Failed => AssessmentCandidateState::Failed,
            _ => AssessmentCandidateState::Unavailable,
        },
        candidate: None,
        verdict: None,
        updated_at: source
            .terminal_at
            .unwrap_or(source.created_at)
            .with_timezone(&Utc),
    })
}

fn data_available(status: &DataStatus) -> bool {
    status.cve_list.availability == "available" && status.cisa_kev.availability == "available"
}

fn read_state(assessment: &upgrade_assessments::Model) -> Result<AssessmentReadState, HttpError> {
    match assessment.status.as_str() {
        "pending" | "processing" => Ok(AssessmentReadState::Processing),
        "completed" => Ok(AssessmentReadState::Completed),
        "failed" => Ok(AssessmentReadState::Failed),
        _ => Err(internal_error("invalid assessment status")),
    }
}

fn selected_candidate_id(candidates: &[AssessmentCandidate]) -> Option<String> {
    candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.selection_state,
                AssessmentSelectionState::Selected
            )
        })
        .map(|candidate| candidate.id.clone())
}

fn candidate_rank(candidate: &AssessmentCandidate) -> (u8, u8) {
    let verdict = match candidate.verdict {
        Some(UpgradeAssessmentVerdict::Recommended) => 0,
        Some(UpgradeAssessmentVerdict::Caution) => 1,
        Some(UpgradeAssessmentVerdict::Unknown) | None => 2,
        Some(UpgradeAssessmentVerdict::Avoid) => 3,
    };
    let distance = match candidate.upgrade_distance {
        UpgradeAssessmentUpgradeDistance::Patch => 0,
        UpgradeAssessmentUpgradeDistance::Minor => 1,
        UpgradeAssessmentUpgradeDistance::Major => 2,
        UpgradeAssessmentUpgradeDistance::Unknown => 3,
    };
    (verdict, distance)
}

fn detail_verdict(
    candidates: &[AssessmentCandidate],
    report: Option<&UpgradeAssessmentResult>,
) -> Option<UpgradeAssessmentVerdict> {
    candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.selection_state,
                AssessmentSelectionState::Selected
            )
        })
        .and_then(|candidate| candidate.verdict.clone())
        .or_else(|| report.map(|report| report.verdict.clone()))
}

fn candidate_state(state: AssessmentReadState, count: usize) -> AssessmentCandidateState {
    match state {
        AssessmentReadState::Completed if count == 0 => AssessmentCandidateState::NoCandidate,
        AssessmentReadState::Completed => AssessmentCandidateState::Available,
        AssessmentReadState::Processing => AssessmentCandidateState::Processing,
        AssessmentReadState::Failed => AssessmentCandidateState::Failed,
        AssessmentReadState::NotAssessed | AssessmentReadState::Unavailable => {
            AssessmentCandidateState::Unavailable
        }
    }
}

fn candidate_read_models(report: &UpgradeAssessmentResult) -> Vec<AssessmentCandidate> {
    let name = report
        .input
        .vulnerable_package
        .as_ref()
        .map(|package| package.name.as_str());
    let baseline = report
        .input
        .vulnerable_package
        .as_ref()
        .map(|package| package.version.as_str());
    report
        .candidate_versions
        .iter()
        .map(|candidate| {
            let purl = name
                .and_then(|name| NpmPackageName::parse(name.to_owned()).ok())
                .map_or_else(
                    || format!("assessment:{}:candidate:{}", report.id, candidate.version),
                    |name| PackageVersion::from_npm(&name, &candidate.version).purl(),
                );
            let compatibility = baseline
                .and_then(|baseline| semver::Version::parse(baseline).ok())
                .zip(semver::Version::parse(&candidate.version).ok())
                .map_or(
                    AssessmentSemverCompatibility::Unknown,
                    |(baseline, candidate)| match api_compatibility(&baseline, &candidate) {
                        ApiCompatibility::Compatible => AssessmentSemverCompatibility::Compatible,
                        ApiCompatibility::Incompatible => {
                            AssessmentSemverCompatibility::Incompatible
                        }
                        ApiCompatibility::NoGuarantee => AssessmentSemverCompatibility::NoGuarantee,
                    },
                );
            let selected = candidate.is_requested_candidate;
            let evidence_state = if selected {
                if report.findings.iter().any(|finding| {
                    matches!(finding.effect, UpgradeAssessmentFindingEffect::MissingCheck)
                }) || report.evidence.is_empty()
                {
                    AssessmentEvidenceState::Missing
                } else {
                    AssessmentEvidenceState::Available
                }
            } else {
                AssessmentEvidenceState::Missing
            };
            AssessmentCandidate {
                id: purl,
                version: candidate.version.clone(),
                upgrade_distance: candidate
                    .upgrade_distance
                    .clone()
                    .unwrap_or(UpgradeAssessmentUpgradeDistance::Unknown),
                semver_compatibility: compatibility,
                selection_state: if selected {
                    AssessmentSelectionState::Selected
                } else {
                    AssessmentSelectionState::Available
                },
                verdict: candidate.verdict.clone(),
                major_upgrade_caution: matches!(
                    candidate.upgrade_distance,
                    Some(UpgradeAssessmentUpgradeDistance::Major)
                )
                .then(|| MAJOR_CAUTION.to_owned()),
                evidence_state,
                findings: if selected {
                    report
                        .findings
                        .iter()
                        .take(MAX_SUPPORTING_ITEMS)
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
                findings_truncated: selected && report.findings.len() > MAX_SUPPORTING_ITEMS,
                evidence: if selected {
                    report
                        .evidence
                        .iter()
                        .take(MAX_SUPPORTING_ITEMS)
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
                evidence_truncated: selected && report.evidence.len() > MAX_SUPPORTING_ITEMS,
                caveats: report
                    .caveats
                    .iter()
                    .take(MAX_SUPPORTING_ITEMS)
                    .cloned()
                    .collect(),
                caveats_truncated: report.caveats.len() > MAX_SUPPORTING_ITEMS,
            }
        })
        .collect()
}

fn view_matches(item: &AssessmentQueueItem, view: AssessmentQueueView) -> bool {
    match view {
        AssessmentQueueView::Processing => item.state == AssessmentReadState::Processing,
        AssessmentQueueView::Completed => item.state == AssessmentReadState::Completed,
        AssessmentQueueView::NeedsReview => {
            matches!(
                item.state,
                AssessmentReadState::NotAssessed
                    | AssessmentReadState::Failed
                    | AssessmentReadState::Unavailable
            ) || (item.state == AssessmentReadState::Completed
                && (item.candidate_state == AssessmentCandidateState::NoCandidate
                    || item.verdict != Some(UpgradeAssessmentVerdict::Recommended)
                    || item.candidate.as_ref().is_some_and(|candidate| {
                        candidate.evidence_state != AssessmentEvidenceState::Available
                    })))
        }
    }
}

fn search_matches(item: &AssessmentQueueItem, search: &str) -> bool {
    let search = search.to_lowercase();
    item.source
        .name
        .as_deref()
        .is_some_and(|name| name.to_lowercase().contains(&search))
        || item.source.file_name.to_lowercase().contains(&search)
        || item
            .exposure
            .as_ref()
            .is_some_and(|exposure| exposure.package.name.to_lowercase().contains(&search))
}

fn accumulate_coverage(
    item: &AssessmentQueueItem,
    exposure_count: &mut u32,
    unassessed_count: &mut u32,
    processing_source_count: &mut u32,
    unavailable_source_count: &mut u32,
) {
    if item.exposure.is_some() {
        *exposure_count = exposure_count.saturating_add(1);
        if item.state == AssessmentReadState::NotAssessed {
            *unassessed_count = unassessed_count.saturating_add(1);
        }
    } else {
        match item.state {
            AssessmentReadState::Processing => {
                *processing_source_count = processing_source_count.saturating_add(1);
            }
            AssessmentReadState::Failed | AssessmentReadState::Unavailable => {
                *unavailable_source_count = unavailable_source_count.saturating_add(1);
            }
            _ => {}
        }
    }
}

fn parse_cursor(cursor: Option<&str>) -> Result<(i32, usize), HttpError> {
    let Some(cursor) = cursor else {
        return Ok((0, 0));
    };
    let Some((source, offset)) = cursor.split_once(':') else {
        return Err(HttpError::for_bad_request(
            None,
            "invalid queue cursor".to_owned(),
        ));
    };
    let source = source.parse::<i32>().ok().filter(|value| *value > 0);
    let offset = offset.parse::<usize>().ok();
    match (source, offset) {
        (Some(source), Some(offset)) => Ok((source, offset)),
        _ => Err(HttpError::for_bad_request(
            None,
            "invalid queue cursor".to_owned(),
        )),
    }
}

async fn linked_assessments(
    db: &DatabaseConnection,
    keys: &[String],
) -> Result<HashMap<String, upgrade_assessments::Model>, HttpError> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = upgrade_assessments::Entity::find()
        .filter(upgrade_assessments::Column::TriggerKind.eq("exposure"))
        .filter(upgrade_assessments::Column::TriggerReference.is_in(keys.iter().cloned()))
        .order_by_desc(upgrade_assessments::Column::CreatedAt)
        .order_by_desc(upgrade_assessments::Column::Id)
        .all(db)
        .await
        .map_err(internal_error)?;
    let mut latest = HashMap::new();
    for row in rows {
        latest.entry(row.trigger_reference.clone()).or_insert(row);
    }
    Ok(latest)
}

fn exposure_item(
    source: &package_sources::Model,
    exposure: PackageSourceExposure,
    assessment: Option<upgrade_assessments::Model>,
) -> Result<AssessmentQueueItem, HttpError> {
    let identity = source_identity(source)?;
    let reference = exposure_reference(identity.id, &exposure);
    let (exposure, reachability_truncated) = bound_exposure(exposure);
    let state = assessment
        .as_ref()
        .map(read_state)
        .transpose()?
        .unwrap_or(AssessmentReadState::NotAssessed);
    let report = assessment
        .clone()
        .filter(|_| state == AssessmentReadState::Completed)
        .map(assessment_result)
        .transpose()?;
    let candidates = report
        .as_ref()
        .map(candidate_read_models)
        .unwrap_or_default();
    let candidate = candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.selection_state,
                AssessmentSelectionState::Selected
            )
        })
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .min_by_key(|(index, candidate)| (candidate_rank(candidate), *index))
                .map(|(_, candidate)| candidate)
        })
        .cloned();
    let verdict = candidate
        .as_ref()
        .and_then(|candidate| candidate.verdict.clone());
    let updated_at = assessment.as_ref().map_or_else(
        || {
            source
                .terminal_at
                .unwrap_or(source.created_at)
                .with_timezone(&Utc)
        },
        |row| {
            row.finished_at
                .unwrap_or(row.created_at)
                .with_timezone(&Utc)
        },
    );
    Ok(AssessmentQueueItem {
        id: exposure_key(&reference),
        detail_url: detail_url(&reference),
        source: identity,
        exposure: Some(exposure),
        reachability_truncated,
        state,
        assessment_id: assessment
            .as_ref()
            .map(|row| Uuid::parse_str(&row.id).map_err(internal_error))
            .transpose()?,
        candidate_state: candidate_state(state, candidates.len()),
        candidate,
        verdict,
        updated_at,
    })
}

async fn source_items(
    db: &DatabaseConnection,
    source: &package_sources::Model,
    available: bool,
) -> Result<Vec<AssessmentQueueItem>, HttpError> {
    match source.resolution_status.as_str() {
        "pending" | "processing" => {
            return Ok(vec![source_status_item(
                source,
                AssessmentReadState::Processing,
            )?]);
        }
        "failed" => {
            return Ok(vec![source_status_item(
                source,
                AssessmentReadState::Failed,
            )?]);
        }
        "cancelled" | "deleting" => return Ok(Vec::new()),
        "completed" => {}
        _ => return Err(internal_error("invalid package-source state")),
    }
    if !available {
        return Ok(vec![source_status_item(
            source,
            AssessmentReadState::Unavailable,
        )?]);
    }
    let source_id = Uuid::parse_str(&source.source_id).map_err(internal_error)?;
    let status = lookup_package_source(db, source_id)
        .await?
        .ok_or_else(|| internal_error("package source disappeared during queue read"))?;
    let versions = match status {
        PackageSourceStatus::Completed(value) => value.versioned_packages,
        PackageSourceStatus::CompletedWithWarnings(value) => value.versioned_packages,
        _ => {
            return Ok(vec![source_status_item(
                source,
                AssessmentReadState::Unavailable,
            )?]);
        }
    };
    let exposures = package_source_exposures(db, &versions).await?;
    let keys = exposures
        .iter()
        .map(|exposure| exposure_key(&exposure_reference(source_id, exposure)))
        .collect::<Vec<_>>();
    let mut assessments = linked_assessments(db, &keys).await?;
    exposures
        .into_iter()
        .map(|exposure| {
            let key = exposure_key(&exposure_reference(source_id, &exposure));
            exposure_item(source, exposure, assessments.remove(&key))
        })
        .collect()
}

pub(super) async fn work_queue(
    ctx: &ApiCtx,
    query: AssessmentWorkQueueQuery,
) -> Result<AssessmentWorkQueue, HttpError> {
    let limit = query.limit.unwrap_or(25);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(HttpError::for_bad_request(
            None,
            "limit must be between 1 and 50".to_owned(),
        ));
    }
    if query
        .search
        .as_ref()
        .is_some_and(|search| search.len() > 128)
    {
        return Err(HttpError::for_bad_request(
            None,
            "search is too long".to_owned(),
        ));
    }
    let (start_source, start_offset) = parse_cursor(query.cursor.as_deref())?;
    let db = ctx.db();
    let status = data_status(
        db,
        ctx.cve_list_freshness_threshold(),
        ctx.kev_freshness_threshold(),
        Utc::now(),
    )
    .await?;
    let available = data_available(&status);
    let sources = package_sources::Entity::find()
        .filter(package_sources::Column::Id.gte(start_source))
        .filter(package_sources::Column::ResolutionStatus.ne("deleting"))
        .order_by_asc(package_sources::Column::Id)
        .limit(
            u64::try_from(MAX_SOURCES_PER_PAGE.saturating_add(1))
                .expect("source page size fits u64"),
        )
        .all(db)
        .await
        .map_err(internal_error)?;
    let has_more_sources = sources.len() > MAX_SOURCES_PER_PAGE;
    let mut items = Vec::new();
    let mut exposure_count = 0_u32;
    let mut unassessed_count = 0_u32;
    let mut processing_source_count = 0_u32;
    let mut unavailable_source_count = 0_u32;
    let mut next_cursor = None;
    for source in sources.into_iter().take(MAX_SOURCES_PER_PAGE) {
        let rows = source_items(db, &source, available).await?;
        let offset = if source.id == start_source {
            start_offset.min(rows.len())
        } else {
            0
        };
        for (index, item) in rows.into_iter().enumerate().skip(offset) {
            let visible = view_matches(&item, query.view)
                && query
                    .search
                    .as_deref()
                    .is_none_or(|search| search_matches(&item, search));
            if visible {
                accumulate_coverage(
                    &item,
                    &mut exposure_count,
                    &mut unassessed_count,
                    &mut processing_source_count,
                    &mut unavailable_source_count,
                );
                items.push(item);
            }
            if items.len() >= limit as usize {
                next_cursor = Some(format!("{}:{}", source.id, index.saturating_add(1)));
                break;
            }
        }
        if next_cursor.is_some() {
            break;
        }
        next_cursor = source
            .id
            .checked_add(1)
            .map(|next_id| format!("{next_id}:0"));
    }
    if !has_more_sources
        && next_cursor.as_ref().is_some_and(|cursor| {
            cursor
                .split_once(':')
                .and_then(|(id, _)| id.parse::<i32>().ok())
                .is_some_and(|id| id > start_source)
        })
        && items.len() < limit as usize
    {
        next_cursor = None;
    }
    let complete = next_cursor.is_none()
        && available
        && processing_source_count == 0
        && unavailable_source_count == 0;
    let message = if !available {
        "Exposure coverage is unavailable until CVE List and KEV data are available. The displayed count is not a zero-exposure result."
    } else if processing_source_count > 0 || unavailable_source_count > 0 {
        "Some package sources have not completed resolution, so exposure coverage is incomplete."
    } else if unassessed_count > 0 {
        "Some reachable KEV-linked exposures have not been assessed."
    } else if next_cursor.is_some() {
        "Additional sources remain; these counts cover only the scanned page."
    } else {
        "All exposures in the scanned sources have assessments."
    };
    Ok(AssessmentWorkQueue {
        view: query.view,
        items,
        next_cursor,
        coverage: AssessmentCoverage {
            scope: "scanned-sources".to_owned(),
            exposure_count,
            unassessed_count,
            processing_source_count,
            unavailable_source_count,
            complete,
            message: message.to_owned(),
        },
        data_status: status,
    })
}

pub(super) async fn exposure_detail(
    ctx: &ApiCtx,
    path: AssessmentExposurePathParams,
    query: AssessmentExposureDetailQuery,
) -> Result<AssessmentExposureDetail, HttpError> {
    let candidate_limit = query.candidate_limit.unwrap_or(50);
    if !(1..=100).contains(&candidate_limit) {
        return Err(HttpError::for_bad_request(
            None,
            "candidateLimit must be between 1 and 100".to_owned(),
        ));
    }
    if !is_cve_id(&path.cve_id) {
        return Err(HttpError::for_bad_request(
            None,
            "invalid CVE identifier".to_owned(),
        ));
    }
    let db = ctx.db();
    let source = package_sources::Entity::find()
        .filter(package_sources::Column::SourceId.eq(path.source_id.to_string()))
        .one(db)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| HttpError::for_not_found(None, "unknown package source".to_owned()))?;
    let status = data_status(
        db,
        ctx.cve_list_freshness_threshold(),
        ctx.kev_freshness_threshold(),
        Utc::now(),
    )
    .await?;
    let identity = source_identity(&source)?;
    let reference = AssessmentExposureReference {
        source_id: path.source_id,
        package_id: path.package_id,
        cve_id: path.cve_id,
    };
    let assessment = linked_assessments(db, &[exposure_key(&reference)])
        .await?
        .remove(&exposure_key(&reference));
    let assessment_id = assessment
        .as_ref()
        .map(|row| Uuid::parse_str(&row.id).map_err(internal_error))
        .transpose()?;
    let exposure = if data_available(&status) && source.resolution_status == "completed" {
        let source_status = lookup_package_source(db, path.source_id)
            .await?
            .ok_or_else(|| internal_error("package source disappeared during detail read"))?;
        let versions = match source_status {
            PackageSourceStatus::Completed(value) => value.versioned_packages,
            PackageSourceStatus::CompletedWithWarnings(value) => value.versioned_packages,
            _ => Vec::new(),
        };
        package_source_exposures(db, &versions)
            .await?
            .into_iter()
            .find(|exposure| {
                exposure.package.id == path.package_id && exposure.cve_id == reference.cve_id
            })
    } else {
        None
    };
    if exposure.is_none() && data_available(&status) && source.resolution_status == "completed" {
        return Err(HttpError::for_not_found(
            None,
            "unknown exposure".to_owned(),
        ));
    }
    let mut state = assessment
        .as_ref()
        .map(read_state)
        .transpose()?
        .unwrap_or(AssessmentReadState::NotAssessed);
    if exposure.is_none() {
        state = AssessmentReadState::Unavailable;
    }
    let report = assessment
        .filter(|_| state == AssessmentReadState::Completed)
        .map(assessment_result)
        .transpose()?;
    let all_candidates = report
        .as_ref()
        .map(candidate_read_models)
        .unwrap_or_default();
    let candidate_count = all_candidates.len();
    let candidate_cursor = query.candidate_cursor.unwrap_or(0);
    if candidate_cursor > candidate_count {
        return Err(HttpError::for_bad_request(
            None,
            "candidateCursor is beyond the candidate list".to_owned(),
        ));
    }
    let selected_candidate_id = selected_candidate_id(&all_candidates);
    let verdict = detail_verdict(&all_candidates, report.as_ref());
    let candidates = all_candidates
        .into_iter()
        .skip(candidate_cursor)
        .take(usize::try_from(candidate_limit).expect("candidate limit fits usize"))
        .collect::<Vec<_>>();
    let end = candidate_cursor.saturating_add(candidates.len());
    let next_candidate_cursor = (end < candidate_count).then_some(end);
    let (exposure, reachability_truncated) = exposure.map_or((None, false), |exposure| {
        let (exposure, truncated) = bound_exposure(exposure);
        (Some(exposure), truncated)
    });
    Ok(AssessmentExposureDetail {
        source: identity,
        exposure,
        reachability_truncated,
        state,
        assessment_id,
        candidate_state: candidate_state(state, candidate_count),
        candidates,
        next_candidate_cursor,
        selected_candidate_id,
        verdict,
        summary: report.as_ref().map(|report| report.summary.clone()),
        caveats: report
            .as_ref()
            .map(|report| {
                report
                    .caveats
                    .iter()
                    .take(MAX_SUPPORTING_ITEMS)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        caveats_truncated: report
            .as_ref()
            .is_some_and(|report| report.caveats.len() > MAX_SUPPORTING_ITEMS),
        failure_message: (state == AssessmentReadState::Failed)
            .then(|| "Assessment processing failed.".to_owned()),
        data_status: status,
    })
}

pub(super) async fn validate_exposure_reference(
    db: &DatabaseConnection,
    input: &UpgradeAssessmentInput,
) -> Result<(), HttpError> {
    let Some(reference) = &input.exposure else {
        return Ok(());
    };
    if !is_cve_id(&reference.cve_id) {
        return Err(upgrade_validation_error(
            "exposure.cveId must be a CVE identifier",
        ));
    }
    let source = lookup_package_source(db, reference.source_id)
        .await?
        .ok_or_else(|| upgrade_validation_error("exposure.sourceId is unknown"))?;
    let (source_contents, versions) = match source {
        PackageSourceStatus::Completed(value) => (value.source.contents, value.versioned_packages),
        PackageSourceStatus::CompletedWithWarnings(value) => {
            (value.source.contents, value.versioned_packages)
        }
        _ => {
            return Err(upgrade_validation_error(
                "exposure source has not completed resolution",
            ));
        }
    };
    if source_contents != input.package_source.contents {
        return Err(upgrade_validation_error(
            "packageSource does not match exposure source",
        ));
    }
    let matched = package_source_exposures(db, &versions)
        .await?
        .into_iter()
        .find(|exposure| {
            exposure.package.id == reference.package_id && exposure.cve_id == reference.cve_id
        })
        .ok_or_else(|| {
            upgrade_validation_error("exposure does not identify a reachable KEV-linked package")
        })?;
    let package = input.vulnerable_package.as_ref().ok_or_else(|| {
        upgrade_validation_error("vulnerablePackage is required for exposure assessments")
    })?;
    if package.name != matched.package.name || package.version != matched.package.version {
        return Err(upgrade_validation_error(
            "vulnerablePackage does not match exposure",
        ));
    }
    if !input
        .cve_linkage
        .as_ref()
        .is_some_and(|ids| ids.contains(&reference.cve_id))
        && !input
            .kev_linkage
            .as_ref()
            .is_some_and(|linkage| linkage.cve_ids.contains(&reference.cve_id))
    {
        return Err(upgrade_validation_error(
            "CVE linkage does not match exposure",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_queue_item(
        state: AssessmentReadState,
        candidate_state: AssessmentCandidateState,
        verdict: Option<UpgradeAssessmentVerdict>,
        source_name: Option<&str>,
        file_name: &str,
        package_name: Option<&str>,
    ) -> AssessmentQueueItem {
        AssessmentQueueItem {
            id: "source:test".to_owned(),
            source: AssessmentSourceIdentity {
                id: Uuid::nil(),
                name: source_name.map(str::to_owned),
                file_name: file_name.to_owned(),
                lifecycle: "completed".to_owned(),
            },
            exposure: package_name.map(|package_name| PackageSourceExposure {
                package: VersionedPackage {
                    id: Uuid::from_u64_pair(1, 2),
                    name: package_name.to_owned(),
                    version: "1.2.3".to_owned(),
                    ecosystem: PackageSourceEcosystem::Npm,
                    purl: format!("pkg:npm/{package_name}@1.2.3"),
                    derivations: Vec::new(),
                },
                cve_id: "CVE-2026-1234".to_owned(),
                kev: PackageSourceExposureKevContext {
                    vendor_project: None,
                    product: None,
                    vulnerability_name: None,
                    date_added: None,
                },
            }),
            reachability_truncated: false,
            state,
            assessment_id: None,
            candidate_state,
            candidate: None,
            verdict,
            updated_at: Utc::now(),
            detail_url: "/package-sources/test".to_owned(),
        }
    }

    #[test]
    fn cursor_and_exposure_identity_are_stable_and_validated() {
        assert_eq!(parse_cursor(Some("12:3")).unwrap(), (12, 3));
        parse_cursor(Some("0:3")).unwrap_err();
        parse_cursor(Some("12:-1")).unwrap_err();
        let reference = AssessmentExposureReference {
            source_id: Uuid::nil(),
            package_id: Uuid::from_u64_pair(1, 2),
            cve_id: "CVE-2026-1234".to_owned(),
        };
        assert_eq!(
            exposure_key(&reference),
            "00000000-0000-0000-0000-000000000000:00000000-0000-0001-0000-000000000002:CVE-2026-1234"
        );
    }

    #[test]
    fn queue_views_distinguish_processing_completed_and_needs_review() {
        let mut item = AssessmentQueueItem {
            id: "source:test".to_owned(),
            source: AssessmentSourceIdentity {
                id: Uuid::nil(),
                name: Some("example".to_owned()),
                file_name: "package.json".to_owned(),
                lifecycle: "completed".to_owned(),
            },
            exposure: None,
            reachability_truncated: false,
            state: AssessmentReadState::Processing,
            assessment_id: None,
            candidate_state: AssessmentCandidateState::Processing,
            candidate: None,
            verdict: None,
            updated_at: Utc::now(),
            detail_url: "/package-sources/test".to_owned(),
        };
        assert!(view_matches(&item, AssessmentQueueView::Processing));
        assert!(!view_matches(&item, AssessmentQueueView::Completed));

        item.state = AssessmentReadState::Completed;
        item.candidate_state = AssessmentCandidateState::NoCandidate;
        assert!(view_matches(&item, AssessmentQueueView::Completed));
        assert!(view_matches(&item, AssessmentQueueView::NeedsReview));

        item.candidate_state = AssessmentCandidateState::Available;
        item.verdict = Some(UpgradeAssessmentVerdict::Recommended);
        assert!(!view_matches(&item, AssessmentQueueView::NeedsReview));

        item.state = AssessmentReadState::NotAssessed;
        item.verdict = None;
        assert!(view_matches(&item, AssessmentQueueView::NeedsReview));
    }

    #[test]
    fn coverage_counts_only_visible_items_after_filters() {
        let hidden_processing = test_queue_item(
            AssessmentReadState::Processing,
            AssessmentCandidateState::Processing,
            None,
            Some("hidden-source"),
            "hidden-package.json",
            None,
        );
        let hidden_exposure = test_queue_item(
            AssessmentReadState::NotAssessed,
            AssessmentCandidateState::Unavailable,
            None,
            Some("alpha-source"),
            "alpha-package.json",
            Some("hidden-package"),
        );
        let visible_exposure = test_queue_item(
            AssessmentReadState::NotAssessed,
            AssessmentCandidateState::Unavailable,
            None,
            Some("visible-source"),
            "visible-package.json",
            Some("visible-package"),
        );

        let mut exposure_count = 0;
        let mut unassessed_count = 0;
        let mut processing_source_count = 0;
        let mut unavailable_source_count = 0;

        for item in [&hidden_processing, &hidden_exposure, &visible_exposure] {
            let visible = view_matches(item, AssessmentQueueView::NeedsReview)
                && search_matches(item, "visible");
            if visible {
                accumulate_coverage(
                    item,
                    &mut exposure_count,
                    &mut unassessed_count,
                    &mut processing_source_count,
                    &mut unavailable_source_count,
                );
            }
        }

        assert_eq!(exposure_count, 1);
        assert_eq!(unassessed_count, 1);
        assert_eq!(processing_source_count, 0);
        assert_eq!(unavailable_source_count, 0);
    }

    #[test]
    fn coverage_counts_visible_processing_source_rows() {
        let visible_processing = test_queue_item(
            AssessmentReadState::Processing,
            AssessmentCandidateState::Processing,
            None,
            Some("processing-source"),
            "processing-package.json",
            None,
        );

        let mut exposure_count = 0;
        let mut unassessed_count = 0;
        let mut processing_source_count = 0;
        let mut unavailable_source_count = 0;

        if view_matches(&visible_processing, AssessmentQueueView::Processing) {
            accumulate_coverage(
                &visible_processing,
                &mut exposure_count,
                &mut unassessed_count,
                &mut processing_source_count,
                &mut unavailable_source_count,
            );
        }

        assert_eq!(exposure_count, 0);
        assert_eq!(unassessed_count, 0);
        assert_eq!(processing_source_count, 1);
        assert_eq!(unavailable_source_count, 0);
    }

    #[test]
    fn coverage_counts_visible_unavailable_source_rows() {
        let visible_unavailable = test_queue_item(
            AssessmentReadState::Unavailable,
            AssessmentCandidateState::Unavailable,
            None,
            Some("unavailable-source"),
            "unavailable-package.json",
            None,
        );

        let mut exposure_count = 0;
        let mut unassessed_count = 0;
        let mut processing_source_count = 0;
        let mut unavailable_source_count = 0;

        if view_matches(&visible_unavailable, AssessmentQueueView::NeedsReview) {
            accumulate_coverage(
                &visible_unavailable,
                &mut exposure_count,
                &mut unassessed_count,
                &mut processing_source_count,
                &mut unavailable_source_count,
            );
        }

        assert_eq!(exposure_count, 0);
        assert_eq!(unassessed_count, 0);
        assert_eq!(processing_source_count, 0);
        assert_eq!(unavailable_source_count, 1);
    }

    #[test]
    fn discovery_major_candidate_has_no_selection_or_candidate_evidence() {
        let mut report = UpgradeAssessmentResult {
            id: Uuid::nil(),
            status: UpgradeAssessmentWorkflowStatus::Completed,
            input: UpgradeAssessmentInput {
                package_source: UpgradeAssessmentPackageSourceInput {
                    ecosystem: PackageSourceEcosystem::Npm,
                    file_name: "package.json".to_owned(),
                    contents: "{}".to_owned(),
                },
                exposure: None,
                vulnerable_package: Some(UpgradeAssessmentVulnerablePackageInput {
                    name: "example-package".to_owned(),
                    ecosystem: PackageSourceEcosystem::Npm,
                    version: "1.2.3".to_owned(),
                    purl: None,
                }),
                cve_linkage: None,
                kev_linkage: None,
                candidate_version: None,
            },
            verdict: UpgradeAssessmentVerdict::Caution,
            summary: "review major upgrade".to_owned(),
            findings: Vec::new(),
            evidence: Vec::new(),
            caveats: Vec::new(),
            candidate_versions: vec![UpgradeAssessmentCandidateVersion {
                version: "2.0.0".to_owned(),
                is_requested_candidate: false,
                release_timestamp: None,
                upgrade_distance: Some(UpgradeAssessmentUpgradeDistance::Major),
                verdict: Some(UpgradeAssessmentVerdict::Caution),
            }],
            assessed_at: Utc::now(),
        };
        let candidates = candidate_read_models(&report);
        assert_eq!(candidates[0].id, "pkg:npm/example-package@2.0.0");
        assert_eq!(
            candidates[0].semver_compatibility,
            AssessmentSemverCompatibility::Incompatible
        );
        assert_eq!(
            candidates[0].selection_state,
            AssessmentSelectionState::Available
        );
        assert_eq!(
            candidates[0].evidence_state,
            AssessmentEvidenceState::Missing
        );
        assert!(candidates[0].major_upgrade_caution.is_some());
        assert!(selected_candidate_id(&candidates).is_none());
        assert_eq!(
            detail_verdict(&candidates, Some(&report)),
            Some(UpgradeAssessmentVerdict::Caution)
        );
        assert_eq!(
            candidate_state(AssessmentReadState::Completed, 0),
            AssessmentCandidateState::NoCandidate
        );

        report.input.vulnerable_package.as_mut().unwrap().version = "0.5.0".to_owned();
        report.candidate_versions[0].version = "0.5.1".to_owned();
        report.candidate_versions[0].upgrade_distance =
            Some(UpgradeAssessmentUpgradeDistance::Patch);
        report.candidate_versions[0].is_requested_candidate = true;
        let candidates = candidate_read_models(&report);
        assert_eq!(
            candidates[0].semver_compatibility,
            AssessmentSemverCompatibility::NoGuarantee
        );
        assert_eq!(
            candidates[0].selection_state,
            AssessmentSelectionState::Selected
        );
        assert_eq!(
            selected_candidate_id(&candidates).as_deref(),
            Some("pkg:npm/example-package@0.5.1")
        );
        assert_eq!(
            detail_verdict(&candidates, Some(&report)),
            candidates[0].verdict.clone()
        );
    }
}
