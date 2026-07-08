//! CVE List repository file ingestion.

use crate::cve::{
    git::{CommitSha, CveListGit, CveListGitError},
    record::{CveRecordParseError, ParsedCveRecord, parse_cve_record},
};
use camino::Utf8PathBuf;

/// A CVE List record parsed from a repository file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCveListFile {
    /// Path to the CVE record file within the CVE List repository.
    pub path: Utf8PathBuf,
    /// Parsed CVE record content.
    pub record: ParsedCveRecord,
}

/// Parse every CVE JSON record present at `commit`.
pub async fn parse_all_cve_files<G>(
    git: &G,
    commit: &CommitSha,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
{
    let paths = git
        .all_cve_files(commit)
        .await
        .map_err(CveListRepositoryError::Git)?;

    parse_cve_files(git, commit, paths).await
}

/// Parse CVE JSON records that were added, modified, or renamed between commits.
pub async fn parse_changed_cve_files<G>(
    git: &G,
    old: &CommitSha,
    new: &CommitSha,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
{
    let paths = git
        .changed_cve_files(old, new)
        .await
        .map_err(CveListRepositoryError::Git)?;

    parse_cve_files(git, new, paths).await
}

async fn parse_cve_files<G>(
    git: &G,
    commit: &CommitSha,
    paths: Vec<Utf8PathBuf>,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
{
    let mut records = Vec::with_capacity(paths.len());

    for path in paths {
        let contents = git
            .read_file_at_commit(commit, &path)
            .await
            .map_err(CveListRepositoryError::Git)?;
        let record = parse_cve_record(contents.as_bytes())
            .map_err(|err| CveListRepositoryError::Record(path.clone(), err))?;

        records.push(ParsedCveListFile { path, record });
    }

    Ok(records)
}

/// Failure while ingesting CVE List repository files.
#[derive(Debug)]
pub enum CveListRepositoryError {
    /// Git access failed.
    Git(CveListGitError),
    /// A CVE record file could not be parsed.
    Record(Utf8PathBuf, CveRecordParseError),
}

impl std::fmt::Display for CveListRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(_) => write!(f, "failed to read CVE List repository files"),
            Self::Record(path, _) => write!(f, "failed to parse CVE List record {path}"),
        }
    }
}

impl std::error::Error for CveListRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Git(err) => Some(err),
            Self::Record(_, err) => Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockCveListGit {
        all_paths: Vec<Utf8PathBuf>,
        changed_paths: Vec<Utf8PathBuf>,
        files: HashMap<Utf8PathBuf, String>,
    }

    #[async_trait]
    impl CveListGit for MockCveListGit {
        async fn ensure_checkout(&self) -> Result<(), CveListGitError> {
            Ok(())
        }

        async fn fetch(&self) -> Result<(), CveListGitError> {
            Ok(())
        }

        async fn resolve_ref(&self, _rev: &str) -> Result<CommitSha, CveListGitError> {
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567")
        }

        async fn changed_cve_files(
            &self,
            _old: &CommitSha,
            _new: &CommitSha,
        ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
            Ok(self.changed_paths.clone())
        }

        async fn all_cve_files(
            &self,
            _commit: &CommitSha,
        ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
            Ok(self.all_paths.clone())
        }

        async fn read_file_at_commit(
            &self,
            _commit: &CommitSha,
            path: &camino::Utf8Path,
        ) -> Result<String, CveListGitError> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| CveListGitError::NonUtf8Path(format!("missing test file {path}")))
        }
    }

    #[test]
    fn parse_all_cve_files_reads_all_repository_records() {
        let git = MockCveListGit {
            all_paths: vec![
                Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json"),
                Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json"),
            ],
            files: HashMap::from([
                (
                    Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json"),
                    cve_record("CVE-2025-1000"),
                ),
                (
                    Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json"),
                    cve_record("CVE-2026-1000"),
                ),
            ]),
            ..Default::default()
        };
        let commit =
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567").expect("valid commit SHA");
        let records = run_async(parse_all_cve_files(&git, &commit)).expect("records should parse");

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].record.cve_id.as_str(), "CVE-2025-1000");
        assert_eq!(records[1].record.cve_id.as_str(), "CVE-2026-1000");
    }

    #[test]
    fn parse_changed_cve_files_reads_records_from_new_commit() {
        let git = MockCveListGit {
            changed_paths: vec![Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json")],
            files: HashMap::from([(
                Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json"),
                cve_record("CVE-2026-1000"),
            )]),
            ..Default::default()
        };
        let old = CommitSha::parse("0123456789abcdef0123456789abcdef01234567")
            .expect("valid old commit SHA");
        let new = CommitSha::parse("fedcba9876543210fedcba9876543210fedcba98")
            .expect("valid new commit SHA");
        let records =
            run_async(parse_changed_cve_files(&git, &old, &new)).expect("records should parse");

        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].path,
            Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json")
        );
        assert_eq!(records[0].record.cve_id.as_str(), "CVE-2026-1000");
    }

    #[test]
    fn parse_cve_files_reports_the_malformed_record_path() {
        let path = Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json");
        let git = MockCveListGit {
            all_paths: vec![path.clone()],
            files: HashMap::from([(path.clone(), "{".to_owned())]),
            ..Default::default()
        };
        let commit =
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567").expect("valid commit SHA");
        let err = run_async(parse_all_cve_files(&git, &commit)).expect_err("record should fail");

        assert!(matches!(
            err,
            CveListRepositoryError::Record(error_path, CveRecordParseError::Json(_))
                if error_path == path
        ));
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }

    fn cve_record(cve_id: &str) -> String {
        format!(
            r#"{{
                "dataType": "CVE_RECORD",
                "dataVersion": "5.2",
                "cveMetadata": {{
                    "cveId": "{cve_id}",
                    "state": "PUBLISHED"
                }}
            }}"#
        )
    }
}
