//! Errors and error-handling helpers.

use crate::config::ConfigErrors;
use camino::Utf8PathBuf;
use std::{
    error::Error,
    fmt::{Debug, Display},
};

/// Fatal errors that stop the server from starting or force it to exit.
pub enum FatalError {
    FailedToBuildDropshotServer(dropshot::ApiDescriptionBuildErrors),
    FailedToStartDropshotServer(dropshot::BuildError),
    FailedToBuildTokioRuntime(std::io::Error),
    FailedToConnectToDatabase(sea_orm::DbErr),
    FailedToInitializeLogger(std::io::Error),
    FailedToOpenConfigFile(Utf8PathBuf, std::io::Error),
    FailedToParseConfigFile(Utf8PathBuf, spookey::Error),
    FailedToParseConfigFileFields(Utf8PathBuf, ConfigErrors),
    UnknownServerError(String),
}

// The `Debug` representation for `FatalError` is intended to match the debug printing for
// `anyhow::Error`, with a top-level error and then a series of causes.
impl Debug for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut msg = format!("{}\n", self);

        // If there are causes, then print the "caused by" section.
        if let Some(source) = self.source() {
            msg.push_str("\nCaused by:\n");

            for source in source.sources_iter() {
                msg.push_str(&format!("\t{}\n", source));
            }
        }

        write!(f, "{}", msg)
    }
}

impl Display for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FatalError::FailedToBuildDropshotServer(_) => {
                write!(f, "failed to build dropshot server")
            }
            FatalError::FailedToStartDropshotServer(_) => {
                write!(f, "failed to start dropshot server")
            }
            FatalError::FailedToBuildTokioRuntime(_) => write!(f, "failed to build tokio runtime"),
            FatalError::FailedToConnectToDatabase(_) => write!(f, "failed to connect to database"),
            FatalError::FailedToInitializeLogger(_) => write!(f, "failed to initialize logger"),
            FatalError::FailedToOpenConfigFile(path, _) => {
                write!(f, "failed to open config file '{}'", path)
            }
            FatalError::FailedToParseConfigFile(path, _) => {
                write!(f, "failed to parse config file '{}'", path)
            }
            FatalError::FailedToParseConfigFileFields(path, _) => {
                write!(f, "failed to parse config file '{}'", path)
            }
            FatalError::UnknownServerError(s) => write!(f, "unknown server error: {}", s),
        }
    }
}

impl std::error::Error for FatalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FatalError::FailedToBuildDropshotServer(err) => Some(err),
            FatalError::FailedToStartDropshotServer(err) => Some(err),
            FatalError::FailedToBuildTokioRuntime(err) => Some(err),
            FatalError::FailedToConnectToDatabase(err) => Some(err),
            FatalError::FailedToInitializeLogger(err) => Some(err),
            FatalError::FailedToOpenConfigFile(_, err) => Some(err),
            FatalError::FailedToParseConfigFile(_, err) => Some(err),
            FatalError::FailedToParseConfigFileFields(_, err) => Some(err),
            FatalError::UnknownServerError(_) => None,
        }
    }
}

/// A generic iterator over causes of an error.
pub struct ErrorSourceIter<'a> {
    /// The "current" error the iterator is handling; if empty, the source chain is done.
    current: Option<&'a dyn std::error::Error>,
}

impl<'a> Iterator for ErrorSourceIter<'a> {
    type Item = &'a dyn std::error::Error;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        self.current = self.current.and_then(|e| e.source());
        current
    }
}

/// Extension trait adding `sources_iter` method to all `std::error::Error` types.
pub trait ErrorSourceIterator {
    // We'd prefer to call this "sources," but the standard library has a nightly-only API to do
    // this exact functionality, and it uses that name, so we get a warning about future
    // incompatibility if we use it ourselves. So we have to use this slightly worse name.
    fn sources_iter<'s>(&'s self) -> ErrorSourceIter<'s>;
}

impl<E: std::error::Error> ErrorSourceIterator for E {
    /// Provides an iterator over all sources of an error, _including_ the original error itself.
    ///
    /// To skip the error itself, call `skip(1)` on the iterator.
    fn sources_iter<'s>(&'s self) -> ErrorSourceIter<'s> {
        ErrorSourceIter {
            current: Some(self),
        }
    }
}
