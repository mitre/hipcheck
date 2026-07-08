//! Git access for the CVE List repository.

use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use tokio::process::Command;
use url::Url;

/// Abstraction over the Git operations needed by CVE List ingestion.
#[async_trait]
pub trait CveListGit {
    /// Ensure the CVE List repository exists in the local cache.
    async fn ensure_checkout(&self) -> Result<(), CveListGitError>;

    /// Fetch the configured upstream ref into the local cache.
    async fn fetch(&self) -> Result<(), CveListGitError>;

    /// Resolve a Git revision to a commit SHA.
    async fn resolve_ref(&self, rev: &str) -> Result<CommitSha, CveListGitError>;

    /// List changed CVE JSON files between two commits.
    async fn changed_cve_files(
        &self,
        old: &CommitSha,
        new: &CommitSha,
    ) -> Result<Vec<Utf8PathBuf>, CveListGitError>;

    /// List every CVE JSON file present at a commit.
    async fn all_cve_files(&self, commit: &CommitSha) -> Result<Vec<Utf8PathBuf>, CveListGitError>;

    /// Read a file from the CVE List repository at a commit.
    async fn read_file_at_commit(
        &self,
        commit: &CommitSha,
        path: &Utf8Path,
    ) -> Result<String, CveListGitError>;
}

/// A Git commit SHA.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitSha {
    kind: CommitShaKind,
    value: String,
}

/// The hash algorithm used by a Git commit SHA.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitShaKind {
    /// A SHA-1 Git object ID.
    Sha1,
    /// A SHA-256 Git object ID.
    Sha256,
}

impl CommitSha {
    /// Create a commit SHA from Git command output.
    pub fn parse(output: &str) -> Result<Self, CveListGitError> {
        let sha = output.trim();

        if sha.is_empty() {
            return Err(CveListGitError::EmptyCommitSha);
        }

        if !is_valid_git_commit_sha(sha) {
            return Err(CveListGitError::InvalidCommitSha(sha.to_owned()));
        }

        Ok(Self {
            kind: CommitShaKind::from_sha_len(sha.len()).expect("validated SHA length"),
            value: sha.to_owned(),
        })
    }

    /// Return the hash algorithm used by this SHA.
    pub fn kind(&self) -> CommitShaKind {
        self.kind
    }

    /// Return the SHA as a string slice.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl CommitShaKind {
    fn from_sha_len(len: usize) -> Option<Self> {
        match len {
            40 => Some(Self::Sha1),
            64 => Some(Self::Sha256),
            _ => None,
        }
    }
}

fn is_valid_git_commit_sha(sha: &str) -> bool {
    CommitShaKind::from_sha_len(sha.len()).is_some() && sha.chars().all(|c| c.is_ascii_hexdigit())
}

/// A Git repository reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRef(String);

impl GitRef {
    /// Create a Git repository reference after validating its format.
    ///
    /// This follows `git check-ref-format --allow-onelevel` constraints so
    /// branch names such as `main` are accepted.
    pub fn parse(value: impl Into<String>) -> Result<Self, GitRefParseError> {
        let value = value.into();

        if is_valid_git_ref(&value) {
            Ok(Self(value))
        } else {
            Err(GitRefParseError(value))
        }
    }

    /// Return the Git ref as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GitRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GitRef {
    type Err = GitRefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

fn is_valid_git_ref(value: &str) -> bool {
    if value.is_empty()
        || value == "@"
        || value.starts_with('/')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.contains("//")
        || value.contains("..")
        || value.contains("@{")
    {
        return false;
    }

    value.split('/').all(is_valid_git_ref_component)
}

fn is_valid_git_ref_component(component: &str) -> bool {
    !component.is_empty()
        && !component.starts_with('.')
        && !component.ends_with(".lock")
        && component.chars().all(is_valid_git_ref_char)
}

fn is_valid_git_ref_char(c: char) -> bool {
    c.is_ascii()
        && !c.is_ascii_control()
        && !matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
}

/// Failure to parse a Git repository reference.
#[derive(Debug)]
pub struct GitRefParseError(String);

impl std::fmt::Display for GitRefParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid Git ref: {}", self.0)
    }
}

impl std::error::Error for GitRefParseError {}

/// CVE List Git access backed by the system `git` command.
#[derive(Clone, Debug)]
pub struct GitCliCveListGit {
    git_program: Utf8PathBuf,
    checkout_dir: Utf8PathBuf,
    repository_url: Url,
    repository_ref: GitRef,
}

impl GitCliCveListGit {
    /// Create a CVE List Git client using the `git` command on `PATH`.
    pub fn new(checkout_dir: Utf8PathBuf, repository_url: Url, repository_ref: GitRef) -> Self {
        Self {
            git_program: Utf8PathBuf::from("git"),
            checkout_dir,
            repository_url,
            repository_ref,
        }
    }

    /// Create a CVE List Git client using a specific Git executable.
    pub fn with_git_program(
        git_program: Utf8PathBuf,
        checkout_dir: Utf8PathBuf,
        repository_url: Url,
        repository_ref: GitRef,
    ) -> Self {
        Self {
            git_program,
            checkout_dir,
            repository_url,
            repository_ref,
        }
    }

    /// Return the local checkout directory.
    pub fn checkout_dir(&self) -> &Utf8Path {
        &self.checkout_dir
    }

    async fn run_git<I, S>(&self, args: I) -> Result<String, CveListGitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_command(self.git_program.as_std_path(), args).await
    }

    async fn run_git_in_checkout<I, S>(&self, args: I) -> Result<String, CveListGitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut git_args = vec![
            OsString::from("-C"),
            OsString::from(self.checkout_dir.as_str()),
        ];
        git_args.extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));

        run_command(self.git_program.as_std_path(), git_args).await
    }
}

#[async_trait]
impl CveListGit for GitCliCveListGit {
    async fn ensure_checkout(&self) -> Result<(), CveListGitError> {
        if self.checkout_dir.join(".git").exists() {
            return Ok(());
        }

        if self.checkout_dir.exists() {
            return Err(CveListGitError::InvalidCheckoutDir(
                self.checkout_dir.clone(),
            ));
        }

        if let Some(parent) = self.checkout_dir.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| CveListGitError::CreateCheckoutParent(parent.to_owned(), err))?;
        }

        self.run_git([
            OsStr::new("clone"),
            OsStr::new(self.repository_url.as_str()),
            OsStr::new(self.checkout_dir.as_str()),
        ])
        .await?;

        Ok(())
    }

    async fn fetch(&self) -> Result<(), CveListGitError> {
        self.run_git_in_checkout([
            OsStr::new("fetch"),
            OsStr::new("--prune"),
            OsStr::new("origin"),
            OsStr::new(self.repository_ref.as_str()),
        ])
        .await?;

        Ok(())
    }

    async fn resolve_ref(&self, rev: &str) -> Result<CommitSha, CveListGitError> {
        let output = self
            .run_git_in_checkout([OsStr::new("rev-parse"), OsStr::new(rev)])
            .await?;

        CommitSha::parse(&output)
    }

    async fn changed_cve_files(
        &self,
        old: &CommitSha,
        new: &CommitSha,
    ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
        let output = self
            .run_git_in_checkout([
                OsStr::new("diff"),
                OsStr::new("--name-only"),
                // Deleted files are intentionally skipped here because there
                // is no record content to ingest from the new commit.
                OsStr::new("--diff-filter=AMR"),
                OsStr::new(old.as_str()),
                OsStr::new(new.as_str()),
                OsStr::new("--"),
                OsStr::new("cves/"),
            ])
            .await?;

        parse_changed_cve_files(&output)
    }

    async fn all_cve_files(&self, commit: &CommitSha) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
        let output = self
            .run_git_in_checkout([
                OsStr::new("ls-tree"),
                OsStr::new("-r"),
                OsStr::new("--name-only"),
                OsStr::new(commit.as_str()),
                OsStr::new("--"),
                OsStr::new("cves/"),
            ])
            .await?;

        parse_changed_cve_files(&output)
    }

    async fn read_file_at_commit(
        &self,
        commit: &CommitSha,
        path: &Utf8Path,
    ) -> Result<String, CveListGitError> {
        let rev_path = format!("{}:{path}", commit.as_str());

        self.run_git_in_checkout([OsStr::new("show"), OsStr::new(&rev_path)])
            .await
    }
}

async fn run_command<I, S>(program: &Path, args: I) -> Result<String, CveListGitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(CveListGitError::RunGit)?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(CveListGitError::GitStdoutUtf8)
    } else {
        Err(CveListGitError::GitFailed {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

fn parse_changed_cve_files(output: &str) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
    output
        .lines()
        .filter(|line| line.starts_with("cves/") && line.ends_with(".json"))
        .map(|line| {
            Utf8PathBuf::from_path_buf(line.into())
                .map_err(|path| CveListGitError::NonUtf8Path(path.display().to_string()))
        })
        .collect()
}

/// Failure while accessing the CVE List Git repository.
#[derive(Debug)]
pub enum CveListGitError {
    /// Failed to create the checkout parent directory.
    CreateCheckoutParent(Utf8PathBuf, std::io::Error),
    /// Git resolved a ref to empty output.
    EmptyCommitSha,
    /// The Git command failed.
    GitFailed {
        /// Git process exit status.
        status: std::process::ExitStatus,
        /// Git stderr, decoded lossily for diagnostics.
        stderr: String,
    },
    /// Git produced stdout that was not valid UTF-8.
    GitStdoutUtf8(std::string::FromUtf8Error),
    /// The checkout path already exists but is not a Git repository.
    InvalidCheckoutDir(Utf8PathBuf),
    /// Git resolved a ref to an invalid commit SHA.
    InvalidCommitSha(String),
    /// Git returned a non-UTF-8 path.
    NonUtf8Path(String),
    /// Failed to run the Git command.
    RunGit(std::io::Error),
}

impl std::fmt::Display for CveListGitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateCheckoutParent(path, _) => {
                write!(f, "failed to create CVE List checkout parent {path}")
            }
            Self::EmptyCommitSha => write!(f, "git resolved an empty commit SHA"),
            Self::GitFailed { status, .. } => write!(f, "git command failed with {status}"),
            Self::GitStdoutUtf8(_) => write!(f, "git command stdout was not valid UTF-8"),
            Self::InvalidCheckoutDir(path) => {
                write!(
                    f,
                    "CVE List checkout path exists but is not a Git repository: {path}"
                )
            }
            Self::InvalidCommitSha(sha) => write!(f, "git resolved an invalid commit SHA: {sha}"),
            Self::NonUtf8Path(path) => write!(f, "git returned a non-UTF-8 path: {path}"),
            Self::RunGit(_) => write!(f, "failed to run git command"),
        }
    }
}

impl std::error::Error for CveListGitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateCheckoutParent(_, err) | Self::RunGit(err) => Some(err),
            Self::GitStdoutUtf8(err) => Some(err),
            Self::EmptyCommitSha
            | Self::GitFailed { .. }
            | Self::InvalidCheckoutDir(_)
            | Self::InvalidCommitSha(_)
            | Self::NonUtf8Path(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_sha_parse_trims_git_output() {
        let sha =
            CommitSha::parse("0123456789abcdef0123456789abcdef01234567\n").expect("valid SHA-1");

        assert_eq!(sha.as_str(), "0123456789abcdef0123456789abcdef01234567");
        assert_eq!(sha.kind(), CommitShaKind::Sha1);
    }

    #[test]
    fn commit_sha_parse_accepts_sha256_output() {
        let raw_sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let sha = CommitSha::parse(raw_sha).expect("valid SHA-256");

        assert_eq!(sha.as_str(), raw_sha);
        assert_eq!(sha.kind(), CommitShaKind::Sha256);
    }

    #[test]
    fn commit_sha_parse_rejects_empty_output() {
        assert!(matches!(
            CommitSha::parse("\n"),
            Err(CveListGitError::EmptyCommitSha)
        ));
    }

    #[test]
    fn commit_sha_parse_rejects_short_output() {
        assert!(matches!(
            CommitSha::parse("abc123"),
            Err(CveListGitError::InvalidCommitSha(_))
        ));
    }

    #[test]
    fn commit_sha_parse_rejects_non_hex_output() {
        assert!(matches!(
            CommitSha::parse("0123456789abcdef0123456789abcdef0123456z"),
            Err(CveListGitError::InvalidCommitSha(_))
        ));
    }

    #[test]
    fn git_ref_parse_accepts_one_level_branch() {
        let git_ref = GitRef::parse("main").expect("valid Git ref");

        assert_eq!(git_ref.as_str(), "main");
    }

    #[test]
    fn git_ref_parse_accepts_full_ref() {
        let git_ref = GitRef::parse("refs/heads/main").expect("valid Git ref");

        assert_eq!(git_ref.as_str(), "refs/heads/main");
    }

    #[test]
    fn git_ref_parse_rejects_invalid_refs() {
        for git_ref in [
            "",
            "@",
            "/main",
            "main/",
            "feature//branch",
            "feature..branch",
            "feature.lock",
            "refs/heads/.main",
            "refs/heads/main.",
            "refs/heads/main@{1}",
            "refs/heads/main^",
            "refs/heads/main~1",
            "refs/heads/main:other",
            "refs/heads/main?",
            "refs/heads/main*",
            "refs/heads/main[",
            "refs/heads/main\\other",
            "refs/heads/main branch",
        ] {
            assert!(
                GitRef::parse(git_ref).is_err(),
                "expected invalid Git ref: {git_ref}"
            );
        }
    }

    #[test]
    fn parse_changed_cve_files_keeps_only_cve_json_paths() {
        let files = parse_changed_cve_files(
            "README.md\ncves/2024/1xxx/CVE-2024-1000.json\ncves/2024/notes.txt\n",
        )
        .expect("valid paths");

        assert_eq!(
            files,
            vec![Utf8PathBuf::from("cves/2024/1xxx/CVE-2024-1000.json")]
        );
    }
}
