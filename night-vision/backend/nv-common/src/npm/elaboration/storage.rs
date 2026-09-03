//! Atomic persistence for completed package-source elaboration runs.

use super::{ElaborationResult, PackageVersion, UnsupportedSpecificationKind};
use crate::db::entities::{
    package_source_edges, package_source_warnings, package_sources, package_versions, packages,
};
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection, DbErr,
    EntityTrait as _, QueryFilter as _, TransactionTrait as _,
};
use std::collections::BTreeMap;
use thiserror::Error;

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
    package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            sea_orm::sea_query::Expr::value("failed"),
        )
        .col_expr(
            package_sources::Column::ResolutionError,
            sea_orm::sea_query::Expr::value(error),
        )
        .filter(package_sources::Column::Id.eq(source_id))
        .exec(db)
        .await
        .map_err(ElaborationStorageError::Database)?;
    Ok(())
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
}
