use api::infrastructure::database::migrations::run_migrations;
use sqlx::{
    ConnectOptions, PgPool,
    migrate::{MigrateError, Migrator},
};
use std::borrow::Cow;
use uuid::Uuid;

fn assert_default_startup_rejects_unknown_history(pool: &PgPool) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let directory = std::env::temp_dir().join(format!("ttl-migration-startup-{}", Uuid::now_v7()));
    std::fs::create_dir(&directory).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_api"));
    // Isolate configuration from the caller and its .env files. In particular,
    // do not set IGNORE_MISSING_MIGRATIONS: this must check the real default.
    command
        .env_clear()
        .current_dir(&directory)
        .envs([
            (
                "DATABASE_URL",
                pool.connect_options().to_url_lossy().to_string(),
            ),
            ("REDIS_URL", "redis://127.0.0.1:9".into()),
            ("R2_ACCESS_KEY_ID", "test".into()),
            ("R2_SECRET_ACCESS_KEY", "test".into()),
            ("R2_ENDPOINT", "http://127.0.0.1:9".into()),
            ("R2_BUCKET_NAME", "test".into()),
            ("R2_PUBLIC_URL", "http://127.0.0.1:9".into()),
            ("JWT_SECRET", "test".into()),
            ("ADMIN_EMAIL", "admin@example.com".into()),
            ("ADMIN_PASSWORD_HASH", "test".into()),
            ("APP_BASE_URL", "http://localhost".into()),
            ("ALLOWED_ORIGINS", "http://localhost".into()),
            ("ENABLE_ML_PROCESSING", "false".into()),
            ("ENABLE_ML_WORKER", "false".into()),
            ("ENABLE_PENDING_AUTO_APPROVE", "false".into()),
            ("RUST_LOG", "error".into()),
            ("HOST", "127.0.0.1".into()),
            ("PORT", "0".into()),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for key in ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "ORT_DYLIB_PATH"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    std::fs::remove_dir(directory).unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("20990101000000") && error.contains("previously applied"),
        "{error}"
    );
}

#[derive(Debug, sqlx::FromRow)]
struct UpgradedUpload {
    id: Uuid,
    detected_text: String,
    status: String,
    pin_code: String,
    location_source: String,
    user_id: Option<Uuid>,
    longitude: f64,
    latitude: f64,
}

// sqlx::test creates and removes a separate database for every test. DATABASE_URL
// must point to a disposable PostGIS server with database-creation privileges.
async fn history(pool: &PgPool) -> Vec<(i64, Vec<u8>)> {
    sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn clean_install_and_restart_preserve_history(pool: PgPool) {
    run_migrations(&pool, false).await.unwrap();
    let installed = history(&pool).await;
    assert_eq!(
        installed.len(),
        sqlx::migrate!("./migrations").iter().count()
    );
    run_migrations(&pool, false).await.unwrap();
    assert_eq!(history(&pool).await, installed);
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM readable_letterings(NULL::uuid)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(visible, 0);
}

async fn upgrade_preserves_data(pool: &PgPool, version: i64) {
    let mut previous: Migrator = sqlx::migrate!("./migrations");
    previous.migrations = Cow::Owned(
        previous
            .iter()
            .filter(|m| m.version <= version)
            .cloned()
            .collect(),
    );
    previous.run(pool).await.unwrap();
    let installed = history(pool).await;
    let city: Uuid = sqlx::query_scalar("SELECT id FROM cities WHERE name = 'Bengaluru'")
        .fetch_one(pool)
        .await
        .unwrap();
    let approved = Uuid::now_v7();
    let rejected = Uuid::now_v7();
    for (id, status) in [(approved, "APPROVED"), (rejected, "REJECTED")] {
        sqlx::query("INSERT INTO letterings (id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, location, pin_code, detected_text, status) VALUES ($1, $2, 'Legacy contributor', 'image', 'small', 'medium', 'large', ST_SetSRID(ST_MakePoint(77.59, 12.97), 4326)::geography, '560001', 'Legacy sign', $3)")
            .bind(id).bind(city).bind(status).execute(pool).await.unwrap();
    }
    let owner = if version >= 20260216000001 {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, 'upgrade@example.com', 'test-hash')")
            .bind(id).execute(pool).await.unwrap();
        sqlx::query("UPDATE letterings SET user_id = $1 WHERE id = $2")
            .bind(id)
            .bind(approved)
            .execute(pool)
            .await
            .unwrap();
        Some(id)
    } else {
        None
    };
    let comment = Uuid::now_v7();
    sqlx::query("INSERT INTO comments (id, lettering_id, content, user_ip) VALUES ($1, $2, 'Legacy comment', '192.0.2.1')")
        .bind(comment).bind(approved).execute(pool).await.unwrap();
    let collection = Uuid::now_v7();
    sqlx::query("INSERT INTO collections (id, name, creator_tag) VALUES ($1, 'Legacy collection', 'Legacy contributor')")
        .bind(collection).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO collection_items (collection_id, lettering_id) VALUES ($1, $2)")
        .bind(collection)
        .bind(approved)
        .execute(pool)
        .await
        .unwrap();

    run_migrations(pool, false).await.unwrap();
    assert_eq!(
        &history(pool).await[..installed.len()],
        installed.as_slice()
    );
    let rows: Vec<UpgradedUpload> = sqlx::query_as("SELECT id, detected_text, status, pin_code, location_source, user_id, ST_X(location::geometry) AS longitude, ST_Y(location::geometry) AS latitude FROM letterings ORDER BY status, id")
        .fetch_all(pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    for (row, expected) in rows
        .iter()
        .zip([(approved, "APPROVED", owner), (rejected, "REJECTED", None)])
    {
        assert_eq!((row.id, row.status.as_str(), row.user_id), expected);
        assert_eq!(row.detected_text, "Legacy sign");
        assert_eq!(row.pin_code, "560001");
        assert_eq!(row.location_source, "legacy_unverified");
        assert_eq!((row.longitude, row.latitude), (77.59, 12.97));
    }
    let content: String = sqlx::query_scalar("SELECT content FROM comments WHERE id = $1")
        .bind(comment)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(content, "Legacy comment");
    let item: Uuid =
        sqlx::query_scalar("SELECT lettering_id FROM collection_items WHERE collection_id = $1")
            .bind(collection)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(item, approved);
    let discoverable: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM discoverable_letterings ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(discoverable, vec![approved]);
    if let Some(owner) = owner {
        let session: i64 = sqlx::query_scalar("SELECT session_version FROM users WHERE id = $1")
            .bind(owner)
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(session, 0);
    }
}

#[sqlx::test(migrations = false)]
async fn upgrade_from_original_schema_preserves_records(pool: PgPool) {
    upgrade_preserves_data(&pool, 20260211133709).await;
}

#[sqlx::test(migrations = false)]
async fn upgrade_from_account_schema_preserves_records_and_ownership(pool: PgPool) {
    upgrade_preserves_data(&pool, 20260307000002).await;
}

#[sqlx::test(migrations = false)]
async fn missing_history_requires_explicit_recovery_override(pool: PgPool) {
    run_migrations(&pool, false).await.unwrap();
    sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (20990101000000, 'unknown deployment', true, $1, 0)")
        .bind(vec![1_u8]).execute(&pool).await.unwrap();
    let installed = history(&pool).await;
    assert_default_startup_rejects_unknown_history(&pool);
    assert!(matches!(
        run_migrations(&pool, false).await,
        Err(MigrateError::VersionMissing(20990101000000))
    ));
    assert_eq!(history(&pool).await, installed);
    run_migrations(&pool, true).await.unwrap();
    assert_eq!(history(&pool).await, installed);
}

#[sqlx::test(migrations = false)]
async fn changed_checksum_is_rejected_even_with_recovery_override(pool: PgPool) {
    run_migrations(&pool, false).await.unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = 20260211133709")
        .bind(vec![1_u8])
        .execute(&pool)
        .await
        .unwrap();
    for ignore_missing in [false, true] {
        assert!(matches!(
            run_migrations(&pool, ignore_missing).await,
            Err(MigrateError::VersionMismatch(20260211133709))
        ));
    }
}

#[sqlx::test(migrations = false)]
async fn failed_history_is_rejected_even_with_recovery_override(pool: PgPool) {
    run_migrations(&pool, false).await.unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET success = false WHERE version = 20260211133709")
        .execute(&pool)
        .await
        .unwrap();
    for ignore_missing in [false, true] {
        assert!(matches!(
            run_migrations(&pool, ignore_missing).await,
            Err(MigrateError::Dirty(20260211133709))
        ));
    }
}
