//! Shared errors and error-handling helpers.

use crate::{config::ConfigErrors, secret::SecretFileError};
use camino::Utf8PathBuf;
use std::{
    error::Error as _,
    fmt::{Debug, Display, Write as _},
};

/// Errors that can occur while loading configuration.
pub enum ConfigLoadError {
    FailedToOpenConfigFile(Utf8PathBuf, std::io::Error),
    FailedToParseConfigFile(Utf8PathBuf, spookey::Error),
    FailedToParseConfigFileFields(Utf8PathBuf, ConfigErrors),
    FailedToReadSecretFile(Utf8PathBuf, SecretFileError),
}

// The `Debug` representation is intended to match the debug printing for `anyhow::Error`, with a
// top-level error and then a series of causes.
impl Debug for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut msg = format!("{self}\n");

        if let Some(source) = self.source() {
            msg.push_str("\nCaused by:\n");

            for source in source.sources_iter() {
                let _ = writeln!(msg, "\t{source}");
            }
        }

        write!(f, "{msg}")
    }
}

impl Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedToOpenConfigFile(path, _) => {
                write!(f, "failed to open config file '{path}'")
            }
            Self::FailedToParseConfigFile(path, _) => {
                write!(f, "failed to parse config file '{path}'")
            }
            Self::FailedToParseConfigFileFields(path, _) => {
                write!(f, "failed to parse config file '{path}'")
            }
            Self::FailedToReadSecretFile(path, _) => {
                write!(f, "failed to read secret file '{path}'")
            }
        }
    }
}

impl std::error::Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FailedToOpenConfigFile(_, err) => Some(err),
            Self::FailedToParseConfigFile(_, err) => Some(err),
            Self::FailedToParseConfigFileFields(_, err) => Some(err),
            Self::FailedToReadSecretFile(_, err) => Some(err),
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
    fn sources_iter(&self) -> ErrorSourceIter<'_>;
}

impl<E: std::error::Error> ErrorSourceIterator for E {
    /// Provides an iterator over all sources of an error, _including_ the original error itself.
    ///
    /// To skip the error itself, call `skip(1)` on the iterator.
    fn sources_iter(&self) -> ErrorSourceIter<'_> {
        ErrorSourceIter {
            current: Some(self),
        }
    }
}
