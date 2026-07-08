//! Progress reporting for CVE List sync operations.

/// Progress event emitted while syncing CVE List data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CveListSyncProgress {
    /// Sync-run metadata has been created.
    Started { generation: i64 },
    /// The local Git checkout is being created or verified.
    GitCheckoutStarted,
    /// The configured upstream Git ref is being fetched.
    GitFetchStarted,
    /// The fetched Git ref is being resolved to a commit.
    GitRefResolveStarted,
    /// Previous successful sync metadata is being loaded.
    PreviousSyncLookupStarted,
    /// The repository is being scanned for CVE files to parse.
    CveFileListStarted,
    /// The repository file list has been loaded.
    CveFileListCompleted { records: usize },
    /// One repository CVE file has been parsed.
    CveFileParsed { parsed: usize, total: usize },
    /// Existing database records are being loaded.
    ExistingRecordLookupStarted { records: usize },
    /// Existing database records have been loaded.
    ExistingRecordLookupCompleted { records: usize },
    /// Parsed CVE records are being written to the database.
    RecordWriteStarted { records: usize },
    /// One database write batch has completed.
    RecordWriteBatchCompleted { written: usize, total: usize },
    /// The configured Git ref has already been synced.
    NotModified,
    /// Sync-run metadata is being finalized.
    FinishStarted,
    /// The sync operation has finished.
    Finished,
}

/// Receiver for CVE List sync progress events.
pub trait CveListSyncProgressReporter: Sync {
    /// Report one progress event.
    fn report(&self, progress: CveListSyncProgress);
}

impl<F> CveListSyncProgressReporter for F
where
    F: Fn(CveListSyncProgress) + Sync,
{
    fn report(&self, progress: CveListSyncProgress) {
        self(progress);
    }
}

/// Progress reporter that ignores all events.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopCveListSyncProgress;

impl CveListSyncProgressReporter for NoopCveListSyncProgress {
    fn report(&self, _progress: CveListSyncProgress) {}
}
