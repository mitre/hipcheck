//! Atomic persistence for completed package-source elaboration runs.

use super::{ElaborationResult, PackageVersion, UnsupportedSpecificationKind};
use crate::db::entities::{
    package_source_edges, package_source_warnings, package_sources, package_versions, packages,
};
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection, DbErr,
    EntityTrait as _, QueryFilter as _, QueryOrder as _, TransactionTrait as _,
};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Maximum UTF-8 byte length retained for a source elaboration failure.
///
/// This bounds durable diagnostics and, in turn, the API response that exposes
/// them. The diagnostic is deliberately short because it may contain text from
/// an external registry.
pub const MAX_FAILURE_DIAGNOSTIC_BYTES: usize = 1024;

/// A resolved package version together with every acyclic path reaching it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedPackageVersion {
    pub id: i32,
    pub version: String,
    pub package_url: String,
    pub derivations: Vec<Vec<String>>,
}

/// Read source-scoped package versions and derive deterministic paths from the
/// stored edge graph.
pub async fn persisted_package_versions(
    db: &DatabaseConnection,
    source_id: i32,
) -> Result<Vec<PersistedPackageVersion>, ElaborationStorageError> {
    let versions = package_versions::Entity::find()
        .filter(package_versions::Column::SourceId.eq(source_id))
        .order_by_asc(package_versions::Column::PackageUrl)
        .all(db)
        .await
        .map_err(ElaborationStorageError::Database)?;
    let edges = package_source_edges::Entity::find()
        .filter(package_source_edges::Column::SourceId.eq(source_id))
        .all(db)
        .await
        .map_err(ElaborationStorageError::Database)?;

    let package_urls = versions
        .iter()
        .map(|version| (version.id, version.package_url.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<Option<i32>, BTreeSet<i32>>::new();
    for edge in edges {
        if !package_urls.contains_key(&edge.child_package_version_id)
            || edge
                .parent_package_version_id
                .is_some_and(|parent| !package_urls.contains_key(&parent))
        {
            return Err(ElaborationStorageError::InvalidGraph);
        }
        children
            .entry(edge.parent_package_version_id)
            .or_default()
            .insert(edge.child_package_version_id);
    }
    let children = children
        .into_iter()
        .map(|(parent, children)| {
            let mut children = children.into_iter().collect::<Vec<_>>();
            children.sort_by(|left, right| {
                package_urls[left]
                    .cmp(&package_urls[right])
                    .then_with(|| left.cmp(right))
            });
            (parent, children)
        })
        .collect::<BTreeMap<_, _>>();

    let mut derivations = BTreeMap::<i32, BTreeSet<Vec<i32>>>::new();
    for root in children.get(&None).into_iter().flatten() {
        let path = vec![*root];
        derivations.entry(*root).or_default().insert(path.clone());
        record_child_derivations(*root, path, &children, &mut derivations);
    }

    Ok(versions
        .into_iter()
        .map(|version| PersistedPackageVersion {
            id: version.id,
            version: version.version,
            package_url: version.package_url,
            derivations: derivations
                .remove(&version.id)
                .unwrap_or_default()
                .into_iter()
                .map(|path| {
                    path.into_iter()
                        .map(|id| package_urls[&id].clone())
                        .collect()
                })
                .collect(),
        })
        .collect())
}

fn record_child_derivations(
    parent: i32,
    path: Vec<i32>,
    children: &BTreeMap<Option<i32>, Vec<i32>>,
    derivations: &mut BTreeMap<i32, BTreeSet<Vec<i32>>>,
) {
    let mut pending = vec![(parent, path)];
    while let Some((parent, path)) = pending.pop() {
        for child in children.get(&Some(parent)).into_iter().flatten() {
            if path.contains(child) {
                continue;
            }
            let mut child_path = path.clone();
            child_path.push(*child);
            derivations
                .entry(*child)
                .or_default()
                .insert(child_path.clone());
            pending.push((*child, child_path));
        }
    }
}

/// Store a completed elaboration snapshot, atomically replacing a source's prior snapshot.
pub async fn persist_completed_elaboration(
    db: &DatabaseConnection,
    source_id: i32,
    result: &ElaborationResult,
) -> Result<(), ElaborationStorageError> {
    let transaction = db
        .begin()
        .await
        .map_err(ElaborationStorageError::Database)?;

    package_source_edges::Entity::delete_many()
        .filter(package_source_edges::Column::SourceId.eq(source_id))
        .exec(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;
    package_source_warnings::Entity::delete_many()
        .filter(package_source_warnings::Column::SourceId.eq(source_id))
        .exec(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;
    package_versions::Entity::delete_many()
        .filter(package_versions::Column::SourceId.eq(source_id))
        .exec(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;

    let mut versions = BTreeMap::new();
    for elaborated in &result.packages {
        let package = find_or_insert_package(&transaction, &elaborated.package).await?;
        let version = package_versions::ActiveModel {
            package_id: Set(package.id),
            source_id: Set(source_id),
            version: Set(elaborated.package.version.to_string()),
            package_url: Set(elaborated.package.purl()),
            source_repository: Set(None),
            source_repository_tag: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;
        versions.insert(elaborated.package.clone(), version.id);
    }

    for edge in &result.edges {
        let child = version_id(&versions, &edge.child)?;
        let parent = edge
            .parent
            .as_ref()
            .map(|package| version_id(&versions, package))
            .transpose()?;
        package_source_edges::ActiveModel {
            source_id: Set(source_id),
            parent_package_version_id: Set(parent),
            child_package_version_id: Set(child),
            root_dependency_kind: Set(edge.root_dependency_kind.map(root_kind)),
            declared_dependency: Set(Some(edge.declared_dependency.to_string())),
            declared_specification: Set(Some(edge.declared_specification.to_string())),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;
    }

    for warning in &result.warnings {
        package_source_warnings::ActiveModel {
            source_id: Set(source_id),
            declared_by_purl: Set(warning.declared_by.as_ref().map(PackageVersion::purl)),
            dependency_name: Set(warning.dependency_name.to_string()),
            specification_kind: Set(warning_kind(warning.specification_kind).to_owned()),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;
    }

    package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            sea_orm::sea_query::Expr::value("completed"),
        )
        .col_expr(
            package_sources::Column::ResolutionError,
            sea_orm::sea_query::Expr::value(None::<String>),
        )
        .filter(package_sources::Column::Id.eq(source_id))
        .exec(&transaction)
        .await
        .map_err(ElaborationStorageError::Database)?;

    transaction
        .commit()
        .await
        .map_err(ElaborationStorageError::Database)
}

/// Record a terminal elaboration failure without changing the last published snapshot.
pub async fn record_elaboration_failure(
    db: &DatabaseConnection,
    source_id: i32,
    error: &str,
) -> Result<(), ElaborationStorageError> {
    let diagnostic = bounded_diagnostic(error);
    package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            sea_orm::sea_query::Expr::value("failed"),
        )
        .col_expr(
            package_sources::Column::ResolutionError,
            sea_orm::sea_query::Expr::value(diagnostic),
        )
        .filter(package_sources::Column::Id.eq(source_id))
        .exec(db)
        .await
        .map_err(ElaborationStorageError::Database)?;
    Ok(())
}

/// Return a diagnostic that is safe to persist or expose under the API's
/// bounded diagnostic contract.
pub fn bounded_diagnostic(error: &str) -> String {
    if error.len() <= MAX_FAILURE_DIAGNOSTIC_BYTES {
        return error.to_owned();
    }

    let mut end = MAX_FAILURE_DIAGNOSTIC_BYTES - "...".len();
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &error[..end])
}

async fn find_or_insert_package(
    db: &sea_orm::DatabaseTransaction,
    package: &PackageVersion,
) -> Result<packages::Model, ElaborationStorageError> {
    if let Some(existing) = packages::Entity::find()
        .filter(packages::Column::Name.eq(package.name.to_string()))
        .filter(packages::Column::PackageHost.eq("npm"))
        .one(db)
        .await
        .map_err(ElaborationStorageError::Database)?
    {
        return Ok(existing);
    }
    packages::ActiveModel {
        name: Set(package.name.to_string()),
        package_host: Set("npm".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(ElaborationStorageError::Database)
}

fn version_id(
    versions: &BTreeMap<PackageVersion, i32>,
    package: &PackageVersion,
) -> Result<i32, ElaborationStorageError> {
    versions
        .get(package)
        .copied()
        .ok_or_else(|| ElaborationStorageError::MissingPackage(package.purl()))
}

fn root_kind(kind: super::DependencyKind) -> String {
    match kind {
        super::DependencyKind::Dependencies => "dependencies",
        super::DependencyKind::BundleDependencies => "bundleDependencies",
        super::DependencyKind::DevDependencies => "devDependencies",
        super::DependencyKind::PeerDependencies => "peerDependencies",
        super::DependencyKind::OptionalDependencies => "optionalDependencies",
    }
    .to_owned()
}

fn warning_kind(kind: UnsupportedSpecificationKind) -> &'static str {
    match kind {
        UnsupportedSpecificationKind::File => "file",
        UnsupportedSpecificationKind::Git => "git",
        UnsupportedSpecificationKind::Url => "url",
    }
}

/// Failure while publishing an elaboration snapshot.
#[derive(Debug, Error)]
pub enum ElaborationStorageError {
    #[error("database operation failed")]
    Database(#[source] DbErr),
    #[error("elaboration edge refers to missing package {0}")]
    MissingPackage(String),
    #[error("persisted elaboration graph refers to a version outside its source")]
    InvalidGraph,
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_FAILURE_DIAGNOSTIC_BYTES, bounded_diagnostic, persist_completed_elaboration,
        persisted_package_versions, record_elaboration_failure,
    };
    use crate::{
        db::entities::{package_source_edges, package_versions},
        npm::elaboration::ElaborationResult,
    };
    use sea_orm::{DbBackend, MockDatabase, MockExecResult};

    #[test]
    fn bounds_failure_diagnostics_at_a_utf8_boundary() {
        let diagnostic = bounded_diagnostic(&"🦀".repeat(MAX_FAILURE_DIAGNOSTIC_BYTES));

        assert!(diagnostic.len() <= MAX_FAILURE_DIAGNOSTIC_BYTES);
        assert!(diagnostic.ends_with("..."));
        assert!(diagnostic.is_char_boundary(diagnostic.len()));
    }

    #[tokio::test]
    async fn derives_all_acyclic_paths_in_package_url_order() {
        let versions = [
            (1, "pkg:npm/a@1.0.0"),
            (2, "pkg:npm/b@1.0.0"),
            (3, "pkg:npm/c@1.0.0"),
            (4, "pkg:npm/d@1.0.0"),
        ]
        .into_iter()
        .map(|(id, package_url)| package_versions::Model {
            id,
            package_id: id,
            source_id: 7,
            version: "1.0.0".to_owned(),
            package_url: package_url.to_owned(),
            source_repository: None,
            source_repository_tag: None,
        })
        .collect::<Vec<_>>();
        let edges = [
            (None, 1),
            (None, 2),
            (Some(1), 3),
            (Some(2), 3),
            (Some(3), 4),
            (Some(4), 1),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, (parent_package_version_id, child_package_version_id))| {
                package_source_edges::Model {
                    id: index as i32,
                    source_id: 7,
                    parent_package_version_id,
                    child_package_version_id,
                    root_dependency_kind: None,
                    declared_dependency: None,
                    declared_specification: None,
                }
            },
        )
        .collect::<Vec<_>>();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([versions])
            .append_query_results([edges])
            .into_connection();

        let result = persisted_package_versions(&db, 7).await.unwrap();

        assert_eq!(result[2].package_url, "pkg:npm/c@1.0.0");
        assert_eq!(
            result[2].derivations,
            vec![
                vec!["pkg:npm/a@1.0.0".to_owned(), "pkg:npm/c@1.0.0".to_owned()],
                vec!["pkg:npm/b@1.0.0".to_owned(), "pkg:npm/c@1.0.0".to_owned()],
            ]
        );
        assert_eq!(result[3].derivations.len(), 2);
        assert!(
            result
                .iter()
                .flat_map(|version| &version.derivations)
                .all(|path| {
                    path.iter().collect::<std::collections::BTreeSet<_>>().len() == path.len()
                })
        );
    }

    #[tokio::test]
    async fn replaces_a_snapshot_in_one_transaction() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                4,
            ))
            .into_connection();

        persist_completed_elaboration(
            &db,
            7,
            &ElaborationResult {
                packages: Vec::new(),
                edges: Vec::new(),
                warnings: Vec::new(),
            },
        )
        .await
        .expect("empty snapshot replacement succeeds");

        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .map(|statement| statement.sql)
            .collect::<Vec<_>>();
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("package_source_edges")),
            "{statements:#?}"
        );
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("package_source_warnings")),
            "{statements:#?}"
        );
        assert!(
            statements.iter().any(|sql| sql.contains("package_version")),
            "{statements:#?}"
        );
        assert!(statements.iter().any(|sql| sql.contains("package_sources")));
        assert_eq!(statements.first(), Some(&"BEGIN".to_owned()));
        assert_eq!(statements.last(), Some(&"COMMIT".to_owned()));
    }

    #[tokio::test]
    async fn failure_preserves_the_published_snapshot() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        record_elaboration_failure(&db, 7, "registry request failed")
            .await
            .expect("failure is recorded");

        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .map(|statement| statement.sql)
            .collect::<Vec<_>>();
        assert_eq!(statements.len(), 1);
        assert!(statements[0].contains("package_sources"), "{statements:#?}");
        assert!(!statements[0].contains("DELETE"));
    }
}
