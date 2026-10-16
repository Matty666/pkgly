#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use nr_core::database::migration::run_migrations;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};

#[tokio::test]
async fn npm_catalog_keeps_case_distinct_prerelease_cache_paths() {
    let _guard = crate::test_support::DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = catalog_database(&docker).await;
    seed_catalog(&pool).await;
    let project_id = Uuid::from_u128(3);
    let repository_id = Uuid::from_u128(2);
    for version in ["1.0.0-RC.1", "1.0.0-rc.1"] {
        let cache_path = format!("packages/case-package/case-package-{version}.tgz");
        let meta = ProxyArtifactMeta::builder("case-package", "case-package", &cache_path)
            .version(version)
            .build();
        let mut extra = VersionData::default();
        extra.set_proxy_artifact(&meta).unwrap();
        NewVersion {
            project_id,
            repository_id,
            version: version.into(),
            release_type: ReleaseType::release_type_from_version(version),
            version_path: DatabaseProxyIndexer::normalize_version_path(
                &cache_path,
                "case-package",
                version,
            ),
            publisher: None,
            version_page: None,
            extra,
        }
        .insert(&pool)
        .await
        .expect("case-distinct cached releases do not collide");
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT version, path FROM project_versions WHERE project_id = $1 ORDER BY version",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "1.0.0-RC.1".into(),
                "packages/case-package/case-package-1.0.0-RC.1.tgz".into()
            ),
            (
                "1.0.0-rc.1".into(),
                "packages/case-package/case-package-1.0.0-rc.1.tgz".into()
            ),
        ]
    );
}

async fn catalog_database(docker: &Cli) -> (Container<'_, GenericImage>, PgPool) {
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let url = format!(
        "postgres://postgres:password@127.0.0.1:{}/postgres",
        container.get_host_port_ipv4(5432)
    );
    for _ in 0..60 {
        if let Ok(pool) = PgPoolOptions::new().max_connections(4).connect(&url).await {
            run_migrations(&pool).await.unwrap();
            return (container, pool);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    panic!("PostgreSQL did not become ready");
}

async fn seed_catalog(pool: &PgPool) {
    sqlx::raw_sql(r#"
        INSERT INTO storages (id, name, storage_type, config)
        VALUES ('00000000-0000-0000-0000-000000000001', 'npm-case-test', 'Local', '{}');
        INSERT INTO repositories (id, storage_id, name, repository_type)
        VALUES ('00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000001', 'npm-proxy', 'npm');
        INSERT INTO projects (id, repository_id, key, name, path)
        VALUES ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000002', 'case-package', 'case-package', 'case-package');
    "#).execute(pool).await.unwrap();
}
