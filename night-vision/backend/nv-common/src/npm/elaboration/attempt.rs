//! Shared setup and durable finalization for a claimed package-source attempt.

use super::{
	ElaborationLimits, ElaborationResult, NpmRegistryClient, PackumentProviderError,
	lifecycle::FailureKind,
	storage::{ElaborationStorageError, persist_completed_elaboration, record_elaboration_failure},
};
use crate::npm::package_json::{NpmPackageJson, PackageParseError};
use sea_orm::DatabaseConnection;
use thiserror::Error;
use url::Url;

/// A package-source attempt already claimed and fenced by its generation.
pub struct ClaimedElaboration<'a> {
	db: &'a DatabaseConnection,
	source_id: i32,
	attempt_generation: i32,
}

/// The durable outcome of finalizing a claimed attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Finalization {
	Completed,
	Failed(FailureKind),
	Inactive,
}

impl<'a> ClaimedElaboration<'a> {
	pub fn new(db: &'a DatabaseConnection, source_id: i32, attempt_generation: i32) -> Self {
		Self {
			db,
			source_id,
			attempt_generation,
		}
	}

	/// Parses the stored source and constructs the configured registry client.
	pub fn prepare(
		contents: &[u8],
		registry_url: Url,
		limits: &ElaborationLimits,
		max_packument_bytes: usize,
	) -> Result<(NpmPackageJson, NpmRegistryClient), ElaborationSetupError> {
		let source = NpmPackageJson::parse_package_json(contents)
			.map_err(ElaborationSetupError::InvalidSource)?;
		let client =
			NpmRegistryClient::new(registry_url, max_packument_bytes, limits.request_timeout)
				.map_err(ElaborationSetupError::InvalidRegistry)?;
		Ok((source, client))
	}

	/// Persists an attempt result, treating a cancelled or superseded claim as
	/// already finalized.
	pub async fn finalize(
		&self,
		result: Result<ElaborationResult, FailureKind>,
	) -> Result<Finalization, ElaborationStorageError> {
		let failure = match result {
			Ok(result) => {
				match persist_completed_elaboration(
					self.db,
					self.source_id,
					self.attempt_generation,
					&result,
				)
				.await
				{
					Ok(()) => return Ok(Finalization::Completed),
					Err(ElaborationStorageError::InactiveAttempt) => {
						return Ok(Finalization::Inactive);
					}
					Err(_) => FailureKind::Internal,
				}
			}
			Err(kind) => kind,
		};
		match record_elaboration_failure(self.db, self.source_id, self.attempt_generation, failure)
			.await
		{
			Ok(()) => Ok(Finalization::Failed(failure)),
			Err(ElaborationStorageError::InactiveAttempt) => Ok(Finalization::Inactive),
			Err(error) => Err(error),
		}
	}
}

/// A setup failure that retains its caller-facing cause and durable category.
#[derive(Debug, Error)]
pub enum ElaborationSetupError {
	#[error("stored source is not a valid npm package.json")]
	InvalidSource(#[source] PackageParseError),
	#[error("invalid NPM registry configuration")]
	InvalidRegistry(#[source] PackumentProviderError),
}

impl ElaborationSetupError {
	pub fn failure_kind(&self) -> FailureKind {
		match self {
			Self::InvalidSource(_) => FailureKind::Validation,
			Self::InvalidRegistry(_) => FailureKind::Internal,
		}
	}
}
