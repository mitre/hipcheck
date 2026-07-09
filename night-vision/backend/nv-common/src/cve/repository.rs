//! CVE List repository file ingestion.

use crate::cve::{
    git::{CommitSha, CveListGit, CveListGitError},
    progress::{CveListSyncProgress, CveListSyncProgressReporter, NoopCveListSyncProgress},
    record::{CveRecordParseError, ParsedCveRecord, parse_cve_record},
};
use camino::Utf8PathBuf;
use futures_util::{StreamExt as _, stream};
use tokio::sync::mpsc;

/// Default number of CVE List records to read and parse concurrently.
pub const DEFAULT_CVE_RECORD_PARSE_CONCURRENCY: usize = 32;

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
    parse_all_cve_files_with_progress(git, commit, &NoopCveListSyncProgress).await
}

/// Parse every CVE JSON record present at `commit`, reporting progress.
pub async fn parse_all_cve_files_with_progress<G, P>(
    git: &G,
    commit: &CommitSha,
    progress: &P,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    parse_all_cve_files_with_progress_and_parse_concurrency(
        git,
        commit,
        progress,
        DEFAULT_CVE_RECORD_PARSE_CONCURRENCY,
    )
    .await
}

/// Parse every CVE JSON record present at `commit`, reporting progress.
pub(crate) async fn parse_all_cve_files_with_progress_and_parse_concurrency<G, P>(
    git: &G,
    commit: &CommitSha,
    progress: &P,
    parse_concurrency: usize,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    let paths = list_all_cve_files_with_progress(git, commit, progress).await?;

    parse_cve_files(git, commit, paths, progress, parse_concurrency).await
}

/// List every CVE JSON record present at `commit`, reporting progress.
pub(crate) async fn list_all_cve_files_with_progress<G, P>(
    git: &G,
    commit: &CommitSha,
    progress: &P,
) -> Result<Vec<Utf8PathBuf>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    progress.report(CveListSyncProgress::CveFileListStarted);
    git.all_cve_files(commit)
        .await
        .map_err(CveListRepositoryError::Git)
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
    parse_changed_cve_files_with_progress(git, old, new, &NoopCveListSyncProgress).await
}

/// Parse CVE JSON records added, modified, or renamed between commits, reporting progress.
pub async fn parse_changed_cve_files_with_progress<G, P>(
    git: &G,
    old: &CommitSha,
    new: &CommitSha,
    progress: &P,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    parse_changed_cve_files_with_progress_and_parse_concurrency(
        git,
        old,
        new,
        progress,
        DEFAULT_CVE_RECORD_PARSE_CONCURRENCY,
    )
    .await
}

/// Parse CVE JSON records added, modified, or renamed between commits, reporting progress.
pub(crate) async fn parse_changed_cve_files_with_progress_and_parse_concurrency<G, P>(
    git: &G,
    old: &CommitSha,
    new: &CommitSha,
    progress: &P,
    parse_concurrency: usize,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    let paths = list_changed_cve_files_with_progress(git, old, new, progress).await?;

    parse_cve_files(git, new, paths, progress, parse_concurrency).await
}

/// List CVE JSON records added, modified, or renamed between commits, reporting progress.
pub(crate) async fn list_changed_cve_files_with_progress<G, P>(
    git: &G,
    old: &CommitSha,
    new: &CommitSha,
    progress: &P,
) -> Result<Vec<Utf8PathBuf>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    progress.report(CveListSyncProgress::CveFileListStarted);
    git.changed_cve_files(old, new)
        .await
        .map_err(CveListRepositoryError::Git)
}

async fn parse_cve_files<G, P>(
    git: &G,
    commit: &CommitSha,
    paths: Vec<Utf8PathBuf>,
    progress: &P,
    parse_concurrency: usize,
) -> Result<Vec<ParsedCveListFile>, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    if parse_concurrency == 0 {
        return Err(CveListRepositoryError::InvalidParseConcurrency);
    }

    let total = paths.len();
    progress.report(CveListSyncProgress::CveFileListCompleted { records: total });

    let mut parsed_files = stream::iter(paths.into_iter().enumerate())
        .map(|(index, path)| async move {
            let contents = git
                .read_file_at_commit(commit, &path)
                .await
                .map_err(CveListRepositoryError::Git)?;
            let record_path = path.clone();
            let task_path = path.clone();
            let record = tokio::task::spawn_blocking(move || parse_cve_record(contents.as_bytes()))
                .await
                .map_err(|err| CveListRepositoryError::ParseTask(task_path, err))?
                .map_err(|err| CveListRepositoryError::Record(record_path, err))?;

            Ok((index, ParsedCveListFile { path, record }))
        })
        .buffer_unordered(parse_concurrency);

    let mut records = Vec::with_capacity(total);
    while let Some(parsed_file) = parsed_files.next().await {
        records.push(parsed_file?);
        progress.report(CveListSyncProgress::CveFileParsed {
            parsed: records.len(),
            total,
        });
    }

    records.sort_by_key(|(index, _)| *index);

    Ok(records.into_iter().map(|(_, record)| record).collect())
}

pub(crate) async fn send_cve_files<G, P>(
    git: &G,
    commit: &CommitSha,
    paths: Vec<Utf8PathBuf>,
    progress: &P,
    parse_concurrency: usize,
    sender: mpsc::Sender<ParsedCveListFile>,
) -> Result<usize, CveListRepositoryError>
where
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    if parse_concurrency == 0 {
        return Err(CveListRepositoryError::InvalidParseConcurrency);
    }

    let total = paths.len();
    progress.report(CveListSyncProgress::CveFileListCompleted { records: total });

    let mut parsed_files = stream::iter(paths)
        .map(|path| async move {
            let contents = git
                .read_file_at_commit(commit, &path)
                .await
                .map_err(CveListRepositoryError::Git)?;
            let record_path = path.clone();
            let task_path = path.clone();
            let record = tokio::task::spawn_blocking(move || parse_cve_record(contents.as_bytes()))
                .await
                .map_err(|err| CveListRepositoryError::ParseTask(task_path, err))?
                .map_err(|err| CveListRepositoryError::Record(record_path, err))?;

            Ok(ParsedCveListFile { path, record })
        })
        .buffer_unordered(parse_concurrency);

    let mut parsed: usize = 0;
    while let Some(parsed_file) = parsed_files.next().await {
        let parsed_file = parsed_file?;
        sender
            .send(parsed_file)
            .await
            .map_err(|_| CveListRepositoryError::ParsedRecordReceiverClosed)?;
        parsed = parsed
            .checked_add(1)
            .expect("parsed count cannot exceed listed CVE file count");
        progress.report(CveListSyncProgress::CveFileParsed { parsed, total });
    }

    Ok(parsed)
}

/// Failure while ingesting CVE List repository files.
#[derive(Debug)]
pub enum CveListRepositoryError {
    /// Git access failed.
    Git(CveListGitError),
    /// The configured parse concurrency was zero.
    InvalidParseConcurrency,
    /// The blocking parser task failed.
    ParseTask(Utf8PathBuf, tokio::task::JoinError),
    /// The parsed-record receiver closed before parsing completed.
    ParsedRecordReceiverClosed,
    /// A CVE record file could not be parsed.
    Record(Utf8PathBuf, CveRecordParseError),
}

impl std::fmt::Display for CveListRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(_) => write!(f, "failed to read CVE List repository files"),
            Self::InvalidParseConcurrency => {
                write!(f, "CVE List parse concurrency must be greater than 0")
            }
            Self::ParseTask(path, _) => {
                write!(f, "failed to run CVE List record parser for {path}")
            }
            Self::ParsedRecordReceiverClosed => {
                write!(
                    f,
                    "parsed CVE List record receiver closed before parsing completed"
                )
            }
            Self::Record(path, _) => write!(f, "failed to parse CVE List record {path}"),
        }
    }
}

impl std::error::Error for CveListRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Git(err) => Some(err),
            Self::InvalidParseConcurrency => None,
            Self::ParseTask(_, err) => Some(err),
            Self::ParsedRecordReceiverClosed => None,
            Self::Record(_, err) => Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::time::Duration;

    #[derive(Default)]
    struct MockCveListGit {
        all_paths: Vec<Utf8PathBuf>,
        changed_paths: Vec<Utf8PathBuf>,
        files: HashMap<Utf8PathBuf, String>,
        read_delays: HashMap<Utf8PathBuf, Duration>,
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
            if let Some(delay) = self.read_delays.get(path) {
                tokio::time::sleep(*delay).await;
            }

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
    fn parse_all_cve_files_preserves_repository_order_when_reads_complete_out_of_order() {
        let slow_path = Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json");
        let fast_path = Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json");
        let git = MockCveListGit {
            all_paths: vec![slow_path.clone(), fast_path.clone()],
            files: HashMap::from([
                (slow_path.clone(), cve_record("CVE-2025-1000")),
                (fast_path.clone(), cve_record("CVE-2026-1000")),
            ]),
            read_delays: HashMap::from([(slow_path.clone(), Duration::from_millis(25))]),
            ..Default::default()
        };
        let commit =
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567").expect("valid commit SHA");
        let records = run_async(parse_all_cve_files(&git, &commit)).expect("records should parse");

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].path, slow_path);
        assert_eq!(records[1].path, fast_path);
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

    #[test]
    fn parse_cve_files_rejects_zero_parse_concurrency() {
        let path = Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json");
        let git = MockCveListGit {
            all_paths: vec![path.clone()],
            files: HashMap::from([(path, cve_record("CVE-2026-1000"))]),
            ..Default::default()
        };
        let commit =
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567").expect("valid commit SHA");
        let err = run_async(parse_all_cve_files_with_progress_and_parse_concurrency(
            &git,
            &commit,
            &NoopCveListSyncProgress,
            0,
        ))
        .expect_err("zero parse concurrency should fail");

        assert!(matches!(
            err,
            CveListRepositoryError::InvalidParseConcurrency
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
