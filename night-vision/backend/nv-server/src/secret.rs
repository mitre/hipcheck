//! Helpers for working with secrets provided through configuration.

use camino::{Utf8Path, Utf8PathBuf};
use secrecy::SecretString;
use std::{
    error::Error,
    fmt::{Debug, Display},
    io,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// A configured source for a secret value.
#[derive(Debug)]
pub(crate) enum SecretSource {
    /// The secret value was provided directly in the server configuration.
    Inline(SecretString),
    /// The secret value should be read from a file at startup.
    File(Utf8PathBuf),
}

impl SecretSource {
    pub(crate) fn inline(value: SecretString) -> Self {
        Self::Inline(value)
    }

    pub(crate) fn file(path: Utf8PathBuf) -> Self {
        Self::File(path)
    }

    /// Resolve this source into a concrete secret value and redacted source kind.
    pub(crate) fn resolve(self) -> Result<ResolvedSecret, SecretSourceError> {
        match self {
            SecretSource::Inline(secret) => Ok(ResolvedSecret {
                kind: SecretSourceKind::Inline,
                value: secret,
            }),
            SecretSource::File(path) => {
                let value = read_secret_file(&path).map_err(|error| SecretSourceError {
                    path: path.clone(),
                    error,
                })?;

                Ok(ResolvedSecret {
                    kind: SecretSourceKind::File,
                    value,
                })
            }
        }
    }
}

/// A resolved secret, preserving only a redacted source kind for reporting.
#[derive(Debug)]
pub(crate) struct ResolvedSecret {
    kind: SecretSourceKind,
    value: SecretString,
}

impl ResolvedSecret {
    pub(crate) fn into_parts(self) -> (SecretSourceKind, SecretString) {
        (self.kind, self.value)
    }
}

/// Redacted source kind for a secret value.
#[derive(Debug)]
pub(crate) enum SecretSourceKind {
    Inline,
    File,
}

impl Display for SecretSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretSourceKind::Inline => write!(f, "<redacted inline secret>"),
            SecretSourceKind::File => write!(f, "<redacted file-backed secret>"),
        }
    }
}

/// A secret source failed to resolve.
#[derive(Debug)]
pub(crate) struct SecretSourceError {
    pub(crate) path: Utf8PathBuf,
    pub(crate) error: SecretFileError,
}

/// Failure to read a secret value from a file.
#[derive(Debug)]
pub enum SecretFileError {
    Read(io::Error),
    Metadata(io::Error),
    Empty,
    InsecurePermissions(u32),
    MultipleLines,
}

impl Display for SecretFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretFileError::Read(_) => write!(f, "failed to read secret file"),
            SecretFileError::Metadata(_) => write!(f, "failed to read secret file metadata"),
            SecretFileError::Empty => write!(f, "secret file is empty"),
            SecretFileError::InsecurePermissions(mode) => write!(
                f,
                "secret file permissions are too broad: expected no group or world permissions, got {:o}",
                mode
            ),
            SecretFileError::MultipleLines => write!(f, "secret file contains multiple lines"),
        }
    }
}

impl Error for SecretFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SecretFileError::Read(err) | SecretFileError::Metadata(err) => Some(err),
            SecretFileError::Empty
            | SecretFileError::InsecurePermissions(_)
            | SecretFileError::MultipleLines => None,
        }
    }
}

fn read_secret_file(path: &Utf8Path) -> Result<SecretString, SecretFileError> {
    validate_secret_file_permissions(path)?;

    let contents = std::fs::read_to_string(path).map_err(SecretFileError::Read)?;
    let trimmed = contents.trim_end_matches(['\r', '\n']);

    if trimmed.is_empty() {
        return Err(SecretFileError::Empty);
    }

    if trimmed.lines().count() > 1 {
        return Err(SecretFileError::MultipleLines);
    }

    Ok(trimmed.to_string().into())
}

#[cfg(unix)]
fn validate_secret_file_permissions(path: &Utf8Path) -> Result<(), SecretFileError> {
    let mode = std::fs::metadata(path)
        .map_err(SecretFileError::Metadata)?
        .permissions()
        .mode()
        & 0o777;

    if mode & 0o077 != 0 {
        return Err(SecretFileError::InsecurePermissions(mode));
    }

    Ok(())
}

#[cfg(not(unix))]
fn validate_secret_file_permissions(_path: &Utf8Path) -> Result<(), SecretFileError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static TEST_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempSecretFile {
        path: Utf8PathBuf,
    }

    impl TempSecretFile {
        fn new(contents: &str) -> Self {
            let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("nv-server-secret-test-{}-{id}", std::process::id()));
            fs::write(&path, contents).expect("failed to write test secret file");

            Self {
                path: Utf8PathBuf::from_path_buf(path)
                    .expect("test temp path should be valid UTF-8"),
            }
        }

        fn path(&self) -> &Utf8Path {
            &self.path
        }
    }

    impl Drop for TempSecretFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[cfg(unix)]
    fn set_file_mode(path: &Utf8Path, mode: u32) {
        let permissions = std::os::unix::fs::PermissionsExt::from_mode(mode);
        fs::set_permissions(path, permissions).expect("failed to set test file permissions");
    }

    #[cfg(not(unix))]
    fn set_file_mode(_path: &Utf8Path, _mode: u32) {}

    fn restrict_secret_file_permissions(path: &Utf8Path) {
        set_file_mode(path, 0o600);
    }

    #[test]
    fn file_source_resolves_single_line_secret() {
        let secret_file = TempSecretFile::new("postgres://user:password@localhost:5432/nv\n");
        restrict_secret_file_permissions(secret_file.path());

        let resolved = SecretSource::file(secret_file.path().to_owned())
            .resolve()
            .expect("single-line secret file should resolve");
        let (kind, secret) = resolved.into_parts();

        assert!(matches!(kind, SecretSourceKind::File));
        assert_eq!(
            secret.expose_secret(),
            "postgres://user:password@localhost:5432/nv"
        );
    }

    #[test]
    fn file_source_rejects_empty_secret() {
        let secret_file = TempSecretFile::new("\n");
        restrict_secret_file_permissions(secret_file.path());

        let error = SecretSource::file(secret_file.path().to_owned())
            .resolve()
            .expect_err("empty secret should fail");

        assert_eq!(error.path, secret_file.path);
        assert!(matches!(error.error, SecretFileError::Empty));
    }

    #[test]
    fn file_source_rejects_multiple_lines() {
        let secret_file = TempSecretFile::new("postgres://localhost:5432/nv\nextra\n");
        restrict_secret_file_permissions(secret_file.path());

        let error = SecretSource::file(secret_file.path().to_owned())
            .resolve()
            .expect_err("multiple-line secret should fail");

        assert_eq!(error.path, secret_file.path);
        assert!(matches!(error.error, SecretFileError::MultipleLines));
    }

    #[cfg(unix)]
    #[test]
    fn file_source_rejects_group_or_world_permissions() {
        let secret_file = TempSecretFile::new("postgres://user:password@localhost:5432/nv\n");
        set_file_mode(secret_file.path(), 0o644);

        let error = SecretSource::file(secret_file.path().to_owned())
            .resolve()
            .expect_err("overly-permissive secret file should fail");

        assert_eq!(error.path, secret_file.path);
        assert!(matches!(
            error.error,
            SecretFileError::InsecurePermissions(0o644)
        ));
    }
}
