//! Git access for the CVE List repository.

use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
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

    /// Read files from the CVE List repository at a commit.
    async fn read_files_at_commit(
        &self,
        commit: &CommitSha,
        paths: &[Utf8PathBuf],
    ) -> Result<Vec<(Utf8PathBuf, String)>, CveListGitError> {
        let (sender, mut receiver) = mpsc::channel(paths.len().max(1));
        self.send_files_at_commit(commit, paths, sender).await?;

        let mut files = Vec::with_capacity(paths.len());
        while let Some(file) = receiver.recv().await {
            files.push(file);
        }

        Ok(files)
    }

    /// Read files from the CVE List repository at a commit into a channel.
    async fn send_files_at_commit(
        &self,
        commit: &CommitSha,
        paths: &[Utf8PathBuf],
        sender: mpsc::Sender<(Utf8PathBuf, String)>,
    ) -> Result<(), CveListGitError> {
        for path in paths {
            let contents = self.read_file_at_commit(commit, path).await?;
            sender
                .send((path.clone(), contents))
                .await
                .map_err(|_| CveListGitError::FileReceiverClosed)?;
        }

        Ok(())
    }
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

    async fn run_git_in_checkout_with_stdin_streaming_cat_file<I, S>(
        &self,
        args: I,
        stdin: Vec<u8>,
        paths: &[Utf8PathBuf],
        sender: mpsc::Sender<(Utf8PathBuf, String)>,
    ) -> Result<(), CveListGitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut git_args = vec![
            OsString::from("-C"),
            OsString::from(self.checkout_dir.as_str()),
        ];
        git_args.extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));

        run_command_with_stdin_streaming_cat_file(
            self.git_program.as_std_path(),
            git_args,
            stdin,
            paths,
            sender,
        )
        .await
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
        let files = self
            .read_files_at_commit(commit, &[path.to_owned()])
            .await?;

        Ok(files
            .into_iter()
            .next()
            .expect("single path read returns one file")
            .1)
    }

    async fn read_files_at_commit(
        &self,
        commit: &CommitSha,
        paths: &[Utf8PathBuf],
    ) -> Result<Vec<(Utf8PathBuf, String)>, CveListGitError> {
        let (sender, mut receiver) = mpsc::channel(paths.len().max(1));
        self.send_files_at_commit(commit, paths, sender).await?;

        let mut files = Vec::with_capacity(paths.len());
        while let Some(file) = receiver.recv().await {
            files.push(file);
        }

        Ok(files)
    }

    async fn send_files_at_commit(
        &self,
        commit: &CommitSha,
        paths: &[Utf8PathBuf],
        sender: mpsc::Sender<(Utf8PathBuf, String)>,
    ) -> Result<(), CveListGitError> {
        if paths.is_empty() {
            return Ok(());
        }

        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(commit.as_str().as_bytes());
            input.push(b':');
            input.extend_from_slice(path.as_str().as_bytes());
            input.push(b'\n');
        }

        self.run_git_in_checkout_with_stdin_streaming_cat_file(
            [OsStr::new("cat-file"), OsStr::new("--batch")],
            input,
            paths,
            sender,
        )
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

async fn run_command_with_stdin_streaming_cat_file<I, S>(
    program: &Path,
    args: I,
    stdin: Vec<u8>,
    paths: &[Utf8PathBuf],
    sender: mpsc::Sender<(Utf8PathBuf, String)>,
) -> Result<(), CveListGitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(CveListGitError::RunGit)?;

    let mut child_stdin = child.stdin.take().expect("stdin was piped");
    let child_stdout = child.stdout.take().expect("stdout was piped");
    let mut child_stderr = child.stderr.take().expect("stderr was piped");
    let write_stdin = tokio::spawn(async move {
        child_stdin.write_all(&stdin).await?;
        drop(child_stdin);

        Ok::<(), std::io::Error>(())
    });
    let read_stderr = tokio::spawn(async move {
        let mut stderr = Vec::new();
        child_stderr.read_to_end(&mut stderr).await?;

        Ok::<Vec<u8>, std::io::Error>(stderr)
    });

    let stream_result = stream_cat_file_batch_output(paths, child_stdout, sender).await;
    let status = child.wait().await.map_err(CveListGitError::RunGit)?;
    let stdin_result = write_stdin
        .await
        .map_err(CveListGitError::GitStdinTask)?
        .map_err(CveListGitError::GitStdin);
    let stderr = read_stderr
        .await
        .map_err(CveListGitError::GitStderrTask)?
        .map_err(CveListGitError::GitStderr)?;

    if status.success() {
        stdin_result?;
        stream_result
    } else {
        Err(CveListGitError::GitFailed {
            status,
            stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
        })
    }
}

async fn stream_cat_file_batch_output<R>(
    paths: &[Utf8PathBuf],
    reader: R,
    sender: mpsc::Sender<(Utf8PathBuf, String)>,
) -> Result<(), CveListGitError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);

    for path in paths {
        let mut header = Vec::new();
        let bytes_read = reader
            .read_until(b'\n', &mut header)
            .await
            .map_err(CveListGitError::GitStdout)?;
        if bytes_read == 0 {
            return Err(CveListGitError::CatFileTruncated(path.clone()));
        }
        if header.last() == Some(&b'\n') {
            header.pop();
        }

        let header = String::from_utf8(header).map_err(CveListGitError::GitStdoutUtf8)?;
        let size = parse_cat_file_header(path, &header)?;
        let mut contents = vec![0; size];
        reader
            .read_exact(&mut contents)
            .await
            .map_err(CveListGitError::GitStdout)?;

        let mut separator = [0; 1];
        reader
            .read_exact(&mut separator)
            .await
            .map_err(CveListGitError::GitStdout)?;
        if separator[0] != b'\n' {
            return Err(CveListGitError::InvalidCatFileSeparator(path.clone()));
        }

        let contents = String::from_utf8(contents).map_err(CveListGitError::GitStdoutUtf8)?;
        sender
            .send((path.clone(), contents))
            .await
            .map_err(|_| CveListGitError::FileReceiverClosed)?;
    }

    Ok(())
}

#[cfg(test)]
fn parse_cat_file_batch_output(
    paths: &[Utf8PathBuf],
    output: &[u8],
) -> Result<Vec<(Utf8PathBuf, String)>, CveListGitError> {
    let mut files = Vec::with_capacity(paths.len());
    let mut offset = 0;

    for path in paths {
        let header_end = output[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| offset + position)
            .ok_or_else(|| CveListGitError::CatFileTruncated(path.clone()))?;
        let header = String::from_utf8(output[offset..header_end].to_vec())
            .map_err(CveListGitError::GitStdoutUtf8)?;

        let size = parse_cat_file_header(path, &header)?;

        let content_start = header_end + 1;
        let content_end = content_start
            .checked_add(size)
            .ok_or_else(|| CveListGitError::CatFileTruncated(path.clone()))?;
        let separator = content_end;
        if output.len() <= separator {
            return Err(CveListGitError::CatFileTruncated(path.clone()));
        }
        if output[separator] != b'\n' {
            return Err(CveListGitError::InvalidCatFileSeparator(path.clone()));
        }

        let contents = String::from_utf8(output[content_start..content_end].to_vec())
            .map_err(CveListGitError::GitStdoutUtf8)?;
        files.push((path.clone(), contents));
        offset = separator + 1;
    }

    if offset != output.len() {
        return Err(CveListGitError::CatFileTrailingData);
    }

    Ok(files)
}

fn parse_cat_file_header(path: &Utf8PathBuf, header: &str) -> Result<usize, CveListGitError> {
    if header.ends_with(" missing") {
        return Err(CveListGitError::CatFileMissing(path.clone()));
    }

    let mut parts = header.split(' ');
    let _object_id = parts
        .next()
        .ok_or_else(|| CveListGitError::InvalidCatFileHeader(path.clone(), header.to_owned()))?;
    let object_type = parts
        .next()
        .ok_or_else(|| CveListGitError::InvalidCatFileHeader(path.clone(), header.to_owned()))?;
    let size = parts
        .next()
        .ok_or_else(|| CveListGitError::InvalidCatFileHeader(path.clone(), header.to_owned()))?
        .parse::<usize>()
        .map_err(|_| CveListGitError::InvalidCatFileHeader(path.clone(), header.to_owned()))?;

    if parts.next().is_some() {
        return Err(CveListGitError::InvalidCatFileHeader(
            path.clone(),
            header.to_owned(),
        ));
    }

    if object_type != "blob" {
        return Err(CveListGitError::UnexpectedCatFileObjectType {
            path: path.clone(),
            object_type: object_type.to_owned(),
        });
    }

    Ok(size)
}

fn parse_changed_cve_files(output: &str) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
    output
        .lines()
        .filter(|line| is_cve_record_path(line))
        .map(|line| {
            Utf8PathBuf::from_path_buf(line.into())
                .map_err(|path| CveListGitError::NonUtf8Path(path.display().to_string()))
        })
        .collect()
}

fn is_cve_record_path(path: &str) -> bool {
    let Some((path_without_extension, extension)) = path.rsplit_once('.') else {
        return false;
    };
    if extension != "json" {
        return false;
    }

    let mut components = path_without_extension.split('/');
    let Some("cves") = components.next() else {
        return false;
    };
    let Some(year) = components.next() else {
        return false;
    };
    let Some(bucket) = components.next() else {
        return false;
    };
    let Some(file_stem) = components.next() else {
        return false;
    };
    if components.next().is_some() {
        return false;
    }

    year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
        && bucket
            .strip_suffix("xxx")
            .is_some_and(|prefix| !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()))
        && is_matching_cve_file_stem(file_stem, year)
}

fn is_matching_cve_file_stem(file_stem: &str, year: &str) -> bool {
    let Some(rest) = file_stem.strip_prefix("CVE-") else {
        return false;
    };
    let Some((file_year, sequence)) = rest.split_once('-') else {
        return false;
    };

    file_year == year && sequence.len() >= 4 && sequence.chars().all(|c| c.is_ascii_digit())
}

/// Failure while accessing the CVE List Git repository.
#[derive(Debug)]
pub enum CveListGitError {
    /// Git cat-file returned a missing object response for a requested file.
    CatFileMissing(Utf8PathBuf),
    /// Git cat-file output ended before a requested file was fully decoded.
    CatFileTruncated(Utf8PathBuf),
    /// Git cat-file returned extra data after all requested files were decoded.
    CatFileTrailingData,
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
    /// Failed to read git command stdout.
    GitStdout(std::io::Error),
    /// Failed to read git command stderr.
    GitStderr(std::io::Error),
    /// Failed to join the task reading git stderr.
    GitStderrTask(tokio::task::JoinError),
    /// Failed to write requests to git stdin.
    GitStdin(std::io::Error),
    /// Failed to join the task writing requests to git stdin.
    GitStdinTask(tokio::task::JoinError),
    /// The file receiver closed before all Git output was streamed.
    FileReceiverClosed,
    /// Git cat-file returned an invalid object header.
    InvalidCatFileHeader(Utf8PathBuf, String),
    /// Git cat-file did not place a newline separator after object content.
    InvalidCatFileSeparator(Utf8PathBuf),
    /// The checkout path already exists but is not a Git repository.
    InvalidCheckoutDir(Utf8PathBuf),
    /// Git resolved a ref to an invalid commit SHA.
    InvalidCommitSha(String),
    /// Git returned a non-UTF-8 path.
    NonUtf8Path(String),
    /// Failed to run the Git command.
    RunGit(std::io::Error),
    /// Git cat-file returned an object that was not a blob.
    UnexpectedCatFileObjectType {
        /// Requested path.
        path: Utf8PathBuf,
        /// Object type returned by Git.
        object_type: String,
    },
}

impl std::fmt::Display for CveListGitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CatFileMissing(path) => {
                write!(f, "git cat-file did not find CVE List file {path}")
            }
            Self::CatFileTruncated(path) => {
                write!(f, "git cat-file output ended while reading {path}")
            }
            Self::CatFileTrailingData => {
                write!(f, "git cat-file returned unexpected trailing data")
            }
            Self::CreateCheckoutParent(path, _) => {
                write!(f, "failed to create CVE List checkout parent {path}")
            }
            Self::EmptyCommitSha => write!(f, "git resolved an empty commit SHA"),
            Self::GitFailed { status, stderr } => {
                if stderr.is_empty() {
                    write!(f, "git command failed with {status}")
                } else {
                    write!(f, "git command failed with {status}: {stderr}")
                }
            }
            Self::GitStdoutUtf8(_) => write!(f, "git command stdout was not valid UTF-8"),
            Self::GitStdout(_) => write!(f, "failed to read git command stdout"),
            Self::GitStderr(_) => write!(f, "failed to read git command stderr"),
            Self::GitStderrTask(_) => write!(f, "failed to join git stderr reader task"),
            Self::GitStdin(_) => write!(f, "failed to write git command stdin"),
            Self::GitStdinTask(_) => write!(f, "failed to join git stdin writer task"),
            Self::FileReceiverClosed => write!(f, "CVE List file receiver closed early"),
            Self::InvalidCatFileHeader(path, _) => {
                write!(f, "git cat-file returned an invalid header for {path}")
            }
            Self::InvalidCatFileSeparator(path) => {
                write!(f, "git cat-file returned an invalid separator after {path}")
            }
            Self::InvalidCheckoutDir(path) => {
                write!(
                    f,
                    "CVE List checkout path exists but is not a Git repository: {path}"
                )
            }
            Self::InvalidCommitSha(sha) => write!(f, "git resolved an invalid commit SHA: {sha}"),
            Self::NonUtf8Path(path) => write!(f, "git returned a non-UTF-8 path: {path}"),
            Self::RunGit(_) => write!(f, "failed to run git command"),
            Self::UnexpectedCatFileObjectType { path, object_type } => {
                write!(f, "git cat-file returned {object_type} for {path}")
            }
        }
    }
}

impl std::error::Error for CveListGitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateCheckoutParent(_, err)
            | Self::RunGit(err)
            | Self::GitStdout(err)
            | Self::GitStderr(err)
            | Self::GitStdin(err) => Some(err),
            Self::GitStderrTask(err) => Some(err),
            Self::GitStdinTask(err) => Some(err),
            Self::GitStdoutUtf8(err) => Some(err),
            Self::CatFileMissing(_)
            | Self::CatFileTrailingData
            | Self::CatFileTruncated(_)
            | Self::InvalidCatFileHeader(_, _)
            | Self::InvalidCatFileSeparator(_)
            | Self::UnexpectedCatFileObjectType { .. }
            | Self::EmptyCommitSha
            | Self::FileReceiverClosed
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
            "README.md\n\
             cves/delta.json\n\
             cves/deltaLog.json\n\
             cves/2024/1xxx/CVE-2024-1000.json\n\
             cves/2024/notes.txt\n\
             cves/2024/1xxx/CVE-2023-1000.json\n\
             cves/2024/1xxx/CVE-2024-100.json\n\
             cves/2024/CVE-2024-1000.json\n\
             cves/2024/1xxx/not-a-cve.json\n",
        )
        .expect("valid paths");

        assert_eq!(
            files,
            vec![Utf8PathBuf::from("cves/2024/1xxx/CVE-2024-1000.json")]
        );
    }

    #[test]
    fn git_failed_display_includes_stderr() {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 128")
            .status()
            .expect("test command should run");
        let error = CveListGitError::GitFailed {
            status,
            stderr: "fatal: couldn't find remote ref missing-ref".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "git command failed with exit status: 128: fatal: couldn't find remote ref missing-ref"
        );
    }

    #[test]
    fn parse_cat_file_batch_output_reads_requested_blobs() {
        let paths = vec![
            Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json"),
            Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json"),
        ];
        let first = br#"{"cveMetadata":{"cveId":"CVE-2025-1000"}}"#;
        let second = br#"{"cveMetadata":{"cveId":"CVE-2026-1000"}}"#;
        let output = cat_file_output([(first.as_slice(), "blob"), (second.as_slice(), "blob")]);

        let files =
            parse_cat_file_batch_output(&paths, &output).expect("cat-file output should parse");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, paths[0]);
        assert_eq!(files[0].1.as_bytes(), first);
        assert_eq!(files[1].0, paths[1]);
        assert_eq!(files[1].1.as_bytes(), second);
    }

    #[test]
    fn stream_cat_file_batch_output_sends_requested_blobs() {
        let paths = vec![
            Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json"),
            Utf8PathBuf::from("cves/2026/1xxx/CVE-2026-1000.json"),
        ];
        let first = br#"{"cveMetadata":{"cveId":"CVE-2025-1000"}}"#;
        let second = br#"{"cveMetadata":{"cveId":"CVE-2026-1000"}}"#;
        let output = cat_file_output([(first.as_slice(), "blob"), (second.as_slice(), "blob")]);
        let (mut writer, reader) = tokio::io::duplex(output.len());
        let (sender, mut receiver) = mpsc::channel(paths.len());

        let files = run_async(async {
            let write_output = tokio::spawn(async move {
                writer
                    .write_all(&output)
                    .await
                    .expect("test output should write");
            });
            stream_cat_file_batch_output(&paths, reader, sender)
                .await
                .expect("cat-file output should stream");
            write_output.await.expect("writer task should complete");

            let mut files = Vec::new();
            while let Some(file) = receiver.recv().await {
                files.push(file);
            }
            files
        });

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, paths[0]);
        assert_eq!(files[0].1.as_bytes(), first);
        assert_eq!(files[1].0, paths[1]);
        assert_eq!(files[1].1.as_bytes(), second);
    }

    #[test]
    fn parse_cat_file_batch_output_rejects_missing_objects() {
        let path = Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json");
        let output = b"0123456789abcdef0123456789abcdef01234567 missing\n";

        let err = parse_cat_file_batch_output(std::slice::from_ref(&path), output)
            .expect_err("missing object should fail");

        assert!(matches!(err, CveListGitError::CatFileMissing(error_path) if error_path == path));
    }

    #[test]
    fn parse_cat_file_batch_output_rejects_non_blob_objects() {
        let path = Utf8PathBuf::from("cves/2025/1xxx/CVE-2025-1000.json");
        let output = cat_file_output([(b"tree-content".as_slice(), "tree")]);

        let err = parse_cat_file_batch_output(std::slice::from_ref(&path), &output)
            .expect_err("non-blob object should fail");

        assert!(matches!(
            err,
            CveListGitError::UnexpectedCatFileObjectType { path: error_path, object_type }
                if error_path == path && object_type == "tree"
        ));
    }

    fn cat_file_output<const N: usize>(objects: [(&[u8], &str); N]) -> Vec<u8> {
        let mut output = Vec::new();

        for (contents, object_type) in objects {
            output.extend_from_slice(b"0123456789abcdef0123456789abcdef01234567 ");
            output.extend_from_slice(object_type.as_bytes());
            output.push(b' ');
            output.extend_from_slice(contents.len().to_string().as_bytes());
            output.push(b'\n');
            output.extend_from_slice(contents);
            output.push(b'\n');
        }

        output
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }
}
