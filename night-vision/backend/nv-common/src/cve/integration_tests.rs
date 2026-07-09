use crate::{
    config::Config,
    cve::{
        git::{CommitSha, CveListGit, CveListGitError, GitRef},
        sync::sync_cve_list_once,
    },
    db::{connection, entities::cve_list_records, entities::cve_list_sync_runs},
};
use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait as _, QueryOrder as _};
use secrecy::ExposeSecret as _;
use serde_json::json;
use std::collections::HashMap;
use url::Url;

const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";
const OLD_COMMIT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const NEW_COMMIT_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";
const TEST_CVE_ID: &str = "CVE-2026-9000001";

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn cve_list_sync_writes_records_and_metadata_to_postgres() {
    run_async(async {
        let db = connect_to_integration_database().await;
        clear_cve_list_tables(&db).await;

        let path = cve_path(TEST_CVE_ID);
        let repository_url =
            Url::parse("https://example.test/night-vision/cvelistV5.git").expect("valid URL");
        let repository_ref = GitRef::parse("main").expect("valid Git ref");

        let first_git = MockCveListGit {
            commit: CommitSha::parse(OLD_COMMIT_SHA).expect("valid commit SHA"),
            all_paths: vec![path.clone()],
            changed_paths: Vec::new(),
            files: HashMap::from([(
                path.clone(),
                cve_record(TEST_CVE_ID, "5.2", "initial detail"),
            )]),
        };

        let first_summary =
            sync_cve_list_once(&db, &first_git, &repository_url, &repository_ref, 50)
                .await
                .expect("initial sync should succeed");

        assert_eq!(first_summary.records_seen, 1);
        assert_eq!(first_summary.records_inserted, 1);
        assert_eq!(first_summary.records_updated, 0);

        let stored = cve_list_records::Entity::find_by_id(TEST_CVE_ID.to_owned())
            .one(&db)
            .await
            .expect("record lookup should succeed")
            .expect("record should be stored");
        assert_eq!(stored.record_format_version, "5.2");
        assert_eq!(stored.record["cveMetadata"]["cveId"], TEST_CVE_ID);
        assert_eq!(
            stored.record["containers"]["cna"]["title"],
            "initial detail"
        );
        assert_eq!(
            stored.record["unknownFutureField"],
            json!({
                "kept": true
            })
        );

        let invalid_record = cve_list_records::ActiveModel {
            cve_id: Set("CVE-2026-9000002".to_owned()),
            record_format_version: Set("5.2".to_owned()),
            record: Set(json!({
                "dataType": "CVE_RECORD",
                "dataVersion": "5.2",
                "cveMetadata": {
                    "cveId": "CVE-2026-9000003"
                }
            })),
            first_seen_at: Default::default(),
            last_seen_at: Default::default(),
            updated_at: Default::default(),
        };
        let constraint_error = cve_list_records::Entity::insert(invalid_record)
            .exec(&db)
            .await
            .expect_err("mismatched CVE ID should fail the database check constraint");
        assert!(
            constraint_error.to_string().contains("check")
                || constraint_error.to_string().contains("constraint"),
            "expected check constraint failure, got {constraint_error}"
        );

        let second_git = MockCveListGit {
            commit: CommitSha::parse(NEW_COMMIT_SHA).expect("valid commit SHA"),
            all_paths: Vec::new(),
            changed_paths: vec![path.clone()],
            files: HashMap::from([(path, cve_record(TEST_CVE_ID, "5.3", "updated detail"))]),
        };

        let second_summary =
            sync_cve_list_once(&db, &second_git, &repository_url, &repository_ref, 50)
                .await
                .expect("incremental sync should succeed");

        assert_eq!(second_summary.records_seen, 1);
        assert_eq!(second_summary.records_inserted, 0);
        assert_eq!(second_summary.records_updated, 1);

        let updated = cve_list_records::Entity::find_by_id(TEST_CVE_ID.to_owned())
            .one(&db)
            .await
            .expect("record lookup should succeed")
            .expect("record should still be stored");
        assert_eq!(updated.record_format_version, "5.3");
        assert_eq!(
            updated.record["containers"]["cna"]["title"],
            "updated detail"
        );
        assert_eq!(updated.first_seen_at, stored.first_seen_at);
        assert!(updated.last_seen_at >= stored.last_seen_at);

        let latest_run = cve_list_sync_runs::Entity::find()
            .order_by_desc(cve_list_sync_runs::Column::Generation)
            .one(&db)
            .await
            .expect("sync-run lookup should succeed")
            .expect("sync run should be stored");
        assert_eq!(latest_run.status, "success");
        assert_eq!(
            latest_run.repository_url.as_deref(),
            Some(repository_url.as_str())
        );
        assert_eq!(
            latest_run.repository_ref.as_deref(),
            Some(repository_ref.as_str())
        );
        assert_eq!(latest_run.commit_sha.as_deref(), Some(NEW_COMMIT_SHA));
        assert_eq!(latest_run.records_seen, 1);
        assert_eq!(latest_run.records_inserted, 0);
        assert_eq!(latest_run.records_updated, 1);
        assert!(latest_run.completed_at.is_some());
        assert!(latest_run.error.is_none());

        clear_cve_list_tables(&db).await;
    });
}

struct MockCveListGit {
    commit: CommitSha,
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

    async fn resolve_ref(&self, rev: &str) -> Result<CommitSha, CveListGitError> {
        assert_eq!(rev, "FETCH_HEAD");
        Ok(self.commit.clone())
    }

    async fn changed_cve_files(
        &self,
        old: &CommitSha,
        new: &CommitSha,
    ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
        assert_eq!(old.as_str(), OLD_COMMIT_SHA);
        assert_eq!(new.as_str(), NEW_COMMIT_SHA);
        Ok(self.changed_paths.clone())
    }

    async fn all_cve_files(&self, commit: &CommitSha) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
        assert_eq!(commit, &self.commit);
        Ok(self.all_paths.clone())
    }

    async fn read_file_at_commit(
        &self,
        commit: &CommitSha,
        path: &Utf8Path,
    ) -> Result<String, CveListGitError> {
        assert_eq!(commit, &self.commit);
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| CveListGitError::NonUtf8Path(format!("missing test file {path}")))
    }
}

async fn connect_to_integration_database() -> DatabaseConnection {
    let config_path = integration_config_path();
    let config = Config::parse(&config_path).expect("integration-test config should parse");
    let database_url = config.database_connection().expose_secret();
    assert_disposable_database_url(&database_url);

    connection(&config).await.unwrap_or_else(|error| {
        panic!(
            "{}",
            integration_database_connection_error(&config_path, database_url, &error)
        )
    })
}

fn integration_config_path() -> Utf8PathBuf {
    std::env::var(CONFIG_PATH_ENV).map_or_else(
        |_| {
            let mut path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push(DEFAULT_CONFIG_PATH);
            path
        },
        Utf8PathBuf::from,
    )
}

fn assert_disposable_database_url(database_url: &str) {
    let url = Url::parse(database_url).expect("database URL should parse");
    assert!(
        matches!(url.scheme(), "postgres" | "postgresql"),
        "integration test database connection must use a Postgres URL"
    );

    let database = url.path().trim_start_matches('/');
    assert!(
        database.contains("test") || database.contains("integration"),
        "integration test database name must contain 'test' or 'integration'; got {database:?}"
    );
}

fn integration_database_connection_error(
    config_path: &Utf8Path,
    database_url: &str,
    error: &dyn std::error::Error,
) -> String {
    let database = Url::parse(database_url)
        .ok()
        .map(|url| url.path().trim_start_matches('/').to_owned())
        .filter(|database| !database.is_empty())
        .unwrap_or_else(|| "<unknown>".to_owned());

    format!(
        "Postgres integration database should connect and migrate.\n\
         \n\
         Config path: {config_path}\n\
         Database: {database}\n\
         \n\
         If the database does not exist, create it first:\n\
         \n\
             createdb {database}\n\
         \n\
         To use another disposable Postgres database, set {CONFIG_PATH_ENV} to a \
         server config file with a database-connection value whose database name \
         contains 'test' or 'integration'.\n\
         \n\
         Original error: {error:#}"
    )
}

async fn clear_cve_list_tables(db: &DatabaseConnection) {
    cve_list_sync_runs::Entity::delete_many()
        .exec(db)
        .await
        .expect("sync runs should clear");
    cve_list_records::Entity::delete_many()
        .exec(db)
        .await
        .expect("records should clear");
}

fn cve_path(cve_id: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!("cves/2026/9000xxx/{cve_id}.json"))
}

fn cve_record(cve_id: &str, version: &str, title: &str) -> String {
    format!(
        r#"{{
            "dataType": "CVE_RECORD",
            "dataVersion": "{version}",
            "cveMetadata": {{
                "cveId": "{cve_id}",
                "state": "PUBLISHED"
            }},
            "containers": {{
                "cna": {{
                    "title": "{title}"
                }}
            }},
            "unknownFutureField": {{
                "kept": true
            }}
        }}"#
    )
}

fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build")
        .block_on(future)
}
