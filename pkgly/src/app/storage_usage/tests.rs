// ABOUTME: Tests storage usage refresh scheduling decisions.
// ABOUTME: Keeps hourly refresh behavior stable without running full scheduler loops.
#![allow(clippy::expect_used)]

use chrono::TimeZone;
use nr_core::{
    database::{DatabaseConfig, entities::storage::NewDBStorage, migration::run_migrations},
    storage::StorageName,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

use super::*;
use crate::{repository::NewRepository, test_support::DB_TEST_LOCK};

struct TestDb {
    pool: PgPool,
    port: u16,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
}

impl TestDb {
    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

async fn start_postgres() -> TestDb {
    let docker: &'static Cli = Box::leak(Box::new(Cli::default()));
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");

    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..30 {
        match PgPoolOptions::new().max_connections(4).connect(&url).await {
            Ok(pool) => {
                return TestDb {
                    pool,
                    port,
                    _container: container,
                    _docker: docker,
                };
            }
            Err(err) => {
                last_err = Some(err.into());
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }

    panic!(
        "postgres container did not become ready: {}",
        last_err.unwrap_or_else(|| anyhow::anyhow!("unknown error"))
    );
}

async fn fresh_db() -> TestDb {
    let db = start_postgres().await;
    run_migrations(db.pool()).await.expect("run migrations");
    db
}

async fn insert_local_storage(pool: &PgPool, root: &std::path::Path) -> Uuid {
    let storage_name = StorageName::new("primary".to_string()).expect("storage name");
    let storage = NewDBStorage::new(
        "Local".into(),
        storage_name,
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": root.to_string_lossy()
            }
        }),
    );
    storage
        .insert(pool)
        .await
        .expect("insert storage")
        .expect("storage row")
        .id
}

async fn insert_maven_hosted_repo(pool: &PgPool, storage_id: Uuid) -> Uuid {
    let repo = NewRepository {
        name: "maven-hosted-test".into(),
        uuid: Uuid::new_v4(),
        repository_type: "maven".into(),
        configs: ahash::HashMap::from_iter([(
            "maven".to_string(),
            serde_json::json!({ "type": "Hosted" }),
        )]),
    };
    repo.insert(storage_id, pool).await.expect("insert repo").id
}

async fn build_site(db: &TestDb, root: &std::path::Path) -> crate::app::Pkgly {
    let cfg = DatabaseConfig {
        user: "postgres".into(),
        password: "password".into(),
        database: "postgres".into(),
        host: "127.0.0.1".into(),
        port: Some(db.port),
    };
    crate::app::Pkgly::new(
        crate::app::config::Mode::Debug,
        crate::app::config::SiteSetting::default(),
        crate::app::config::SecuritySettings::default(),
        crate::app::authentication::session::SessionManagerConfig {
            database_location: root.join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: root.join("staging"),
            ..Default::default()
        },
        None,
        cfg,
        Some(root.join("storages")),
    )
    .await
    .expect("create site")
}

#[test]
fn repository_without_previous_usage_refresh_is_due() {
    let now = Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap();

    assert!(storage_usage_refresh_due(now, None));
}

#[test]
fn repository_is_not_due_before_one_hour() {
    let now = Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap();
    let previous = (now - chrono::Duration::minutes(59)).fixed_offset();

    assert!(!storage_usage_refresh_due(now, Some(previous)));
}

#[test]
fn repository_is_due_at_one_hour() {
    let now = Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap();
    let previous = (now - chrono::Duration::hours(1)).fixed_offset();

    assert!(storage_usage_refresh_due(now, Some(previous)));
}

#[tokio::test]
async fn scheduler_tick_refreshes_due_repository_once_per_hour() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let storage_root = root.path().join("storage-data");
    std::fs::create_dir_all(&storage_root).expect("create storage root");
    let storage_id = insert_local_storage(db.pool(), &storage_root).await;
    let repository_id = insert_maven_hosted_repo(db.pool(), storage_id).await;
    let repo_root = storage_root.join(repository_id.to_string());
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    std::fs::write(repo_root.join("artifact.bin"), vec![0u8; 37]).expect("write artifact");

    let site = build_site(&db, root.path()).await;
    let now = Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap();

    let first_summary = storage_usage_scheduler_tick(site.clone(), now)
        .await
        .expect("first scheduler tick");
    let first_row = DBRepository::get_by_id(repository_id, db.pool())
        .await
        .expect("load repository")
        .expect("repository exists");

    assert_eq!(first_summary.due_repositories, 1);
    assert_eq!(first_summary.refreshed, 1);
    assert_eq!(first_row.storage_usage_bytes, Some(37));
    assert!(first_row.storage_usage_updated_at.is_some());

    let second_summary =
        storage_usage_scheduler_tick(site.clone(), now + chrono::Duration::minutes(59))
            .await
            .expect("second scheduler tick");

    assert_eq!(second_summary.due_repositories, 0);
    assert_eq!(second_summary.refreshed, 0);

    site.close().await;
}

#[tokio::test]
async fn scheduler_tick_counts_storage_calculation_errors_as_failed() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().expect("tempdir");
    let storage_root = root.path().join("storage-data");
    std::fs::create_dir_all(&storage_root).expect("create storage root");
    let storage_id = insert_local_storage(db.pool(), &storage_root).await;
    let repository_id = insert_maven_hosted_repo(db.pool(), storage_id).await;
    let repo_root = storage_root.join(repository_id.to_string());
    let inaccessible_dir = repo_root.join("inaccessible");
    std::fs::create_dir_all(&inaccessible_dir).expect("create inaccessible dir");
    std::fs::set_permissions(&inaccessible_dir, std::fs::Permissions::from_mode(0o000))
        .expect("make dir inaccessible");

    let site = build_site(&db, root.path()).await;
    let now = Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap();

    let summary = storage_usage_scheduler_tick(site.clone(), now)
        .await
        .expect("scheduler tick");
    let row = DBRepository::get_by_id(repository_id, db.pool())
        .await
        .expect("load repository")
        .expect("repository exists");

    assert_eq!(summary.due_repositories, 1);
    assert_eq!(summary.refreshed, 0);
    assert_eq!(summary.missing_repository, 0);
    assert_eq!(summary.failed, 1);
    assert_eq!(row.storage_usage_bytes, None);
    assert_eq!(row.storage_usage_updated_at, None);

    std::fs::set_permissions(&inaccessible_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore dir permissions");
    site.close().await;
}
