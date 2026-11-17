use api::{
    infrastructure::database::pool::{create_migration_pool, create_pool},
    limits::Limits,
};
use sqlx::{ConnectOptions, PgPool};
use std::time::{Duration, Instant};

#[sqlx::test]
async fn database_budgets_bound_waits_and_leave_usable_connections(pool: PgPool) {
    let mut url = pool.connect_options().to_url_lossy();
    url.query_pairs_mut()
        .append_pair("options", "-c statement_timeout=0 -c lock_timeout=0");
    let url = url.to_string();
    let mut limits = Limits::bundled().unwrap().database;
    limits.max_connections = 1.try_into().unwrap();
    limits.acquire_timeout_ms = 150.try_into().unwrap();
    limits.statement_timeout_ms = 300.try_into().unwrap();
    limits.lock_timeout_ms = 100.try_into().unwrap();
    let bounded = create_pool(&url, &limits).await.unwrap();
    let held = bounded.acquire().await.unwrap();
    let started = Instant::now();
    let error = bounded.acquire().await.unwrap_err();
    assert!(matches!(error, sqlx::Error::PoolTimedOut));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(held);
    let started = Instant::now();
    let error = sqlx::query("SELECT pg_sleep(3)")
        .execute(&bounded)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("57014")
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&bounded)
            .await
            .unwrap(),
        1
    );
    let mut migration_limits = limits.clone();
    migration_limits.migration_timeout_ms = 1000.try_into().unwrap();
    let migrations = create_migration_pool(&url, &migration_limits)
        .await
        .unwrap();
    sqlx::query("SELECT pg_sleep(0.5)")
        .execute(&migrations)
        .await
        .unwrap();
    assert_eq!(migrations.options().get_max_connections(), 1);
    migrations.close().await;
    let error = sqlx::query("SELECT pg_sleep(0.5)")
        .execute(&bounded)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("57014")
    );
    bounded.close().await;

    limits.max_connections = 2.try_into().unwrap();
    let bounded = create_pool(&url, &limits).await.unwrap();
    sqlx::query("CREATE TABLE budget_probe(id integer)")
        .execute(&bounded)
        .await
        .unwrap();
    let mut held = bounded.begin().await.unwrap();
    sqlx::query("LOCK TABLE budget_probe IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *held)
        .await
        .unwrap();
    let error = sqlx::query("SELECT * FROM budget_probe")
        .execute(&bounded)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("55P03")
    );
    held.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&bounded)
            .await
            .unwrap(),
        1
    );
    bounded.close().await;
}
