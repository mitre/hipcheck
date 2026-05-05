use crate::config::ParseErrors;
use camino::Utf8PathBuf;
use std::fmt::Display;

/// Fatal errors that stop the server from starting or force it to exit.
#[derive(Debug)]
pub enum FatalError {
    FailedToBuildDropshotServer(dropshot::ApiDescriptionBuildErrors),
    FailedToStartDropshotServer(dropshot::BuildError),
    FailedToBuildTokioRuntime(std::io::Error),
    FailedToConnectToDatabase(sea_orm::DbErr),
    FailedToInitializeLogger(std::io::Error),
    FailedToOpenConfigFile(Utf8PathBuf, std::io::Error),
    FailedToParseConfigFile(Utf8PathBuf, spookey::Error),
    FailedToParseConfigFileFields(Utf8PathBuf, ParseErrors),
    UnknownServerError(String),
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
