//! Errors and error-handling helpers.

use camino::Utf8PathBuf;
use nv_common::{
    config::ConfigLoadError, db::DatabaseConnectionError, error::ErrorSourceIterator as _,
    rt::RuntimeBuildError,
};
use std::{
    error::Error as _,
    fmt::{Debug, Display, Write as _},
};

/// Fatal errors that stop the server from starting or force it to exit.
pub enum FatalError {
    FailedToBuildDropshotServer(dropshot::ApiDescriptionBuildErrors),
    FailedToStartDropshotServer(dropshot::BuildError),
    FailedToBuildTokioRuntime(RuntimeBuildError),
    FailedToConnectToDatabase(DatabaseConnectionError),
    FailedToLoadConfig(ConfigLoadError),
    FailedToCreateOpenApiDescFile(Utf8PathBuf, std::io::Error),
    FailedToWriteOpenApiDescFile(Utf8PathBuf, serde_json::Error),
    NoOpenApiDestPathInOpenApiMode(),
    UnknownServerError(String),
}

// The `Debug` representation for `FatalError` is intended to match the debug printing for
// `anyhow::Error`, with a top-level error and then a series of causes.
impl Debug for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut msg = format!("{self}\n");

        // If there are causes, then print the "caused by" section.
        if let Some(source) = self.source() {
            msg.push_str("\nCaused by:\n");

            for source in source.sources_iter() {
                let _ = writeln!(msg, "\t{source}");
            }
        }

        write!(f, "{msg}")
    }
}

impl Display for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedToBuildDropshotServer(_) => {
                write!(f, "failed to build dropshot server")
            }
            Self::FailedToStartDropshotServer(_) => {
                write!(f, "failed to start dropshot server")
            }
            Self::FailedToBuildTokioRuntime(_) => write!(f, "failed to build tokio runtime"),
            Self::FailedToConnectToDatabase(_) => write!(f, "failed to connect to database"),
            Self::FailedToLoadConfig(_) => write!(f, "failed to load configuration"),
            Self::FailedToCreateOpenApiDescFile(path, _) => {
                write!(f, "failed to create OpenAPI Description file '{path}'")
            }
            Self::FailedToWriteOpenApiDescFile(path, _) => {
                write!(f, "failed to write OpenAPI Description file '{path}'")
            }
            Self::NoOpenApiDestPathInOpenApiMode() => {
                write!(
                    f,
                    "cannot write OpenAPI Description with no destination path set in config file"
                )
            }
            Self::UnknownServerError(s) => write!(f, "unknown server error: {s}"),
        }
    }
}

impl std::error::Error for FatalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FailedToBuildDropshotServer(err) => Some(err),
            Self::FailedToStartDropshotServer(err) => Some(err),
            Self::FailedToBuildTokioRuntime(err) => Some(err),
            Self::FailedToConnectToDatabase(err) => Some(err),
            Self::FailedToLoadConfig(err) => Some(err),
            Self::FailedToCreateOpenApiDescFile(_, err) => Some(err),
            Self::FailedToWriteOpenApiDescFile(_, err) => Some(err),
            Self::NoOpenApiDestPathInOpenApiMode() => None,
            Self::UnknownServerError(_) => None,
        }
    }
}

impl From<ConfigLoadError> for FatalError {
    fn from(error: ConfigLoadError) -> Self {
        Self::FailedToLoadConfig(error)
    }
}

impl From<DatabaseConnectionError> for FatalError {
    fn from(error: DatabaseConnectionError) -> Self {
        Self::FailedToConnectToDatabase(error)
    }
}

impl From<RuntimeBuildError> for FatalError {
    fn from(error: RuntimeBuildError) -> Self {
        Self::FailedToBuildTokioRuntime(error)
    }
}
