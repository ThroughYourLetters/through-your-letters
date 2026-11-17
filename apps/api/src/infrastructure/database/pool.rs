use crate::limits::DatabaseLimits;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::{str::FromStr, time::Duration};

pub async fn create_pool(database_url: &str, limits: &DatabaseLimits) -> anyhow::Result<PgPool> {
    connect(
        database_url,
        limits,
        limits.max_connections.get(),
        limits.statement_timeout_ms.get(),
    )
    .await
}

/// Schema changes use a separate one-connection pool with their own statement deadline.
pub async fn create_migration_pool(
    database_url: &str,
    limits: &DatabaseLimits,
) -> anyhow::Result<PgPool> {
    connect(database_url, limits, 1, limits.migration_timeout_ms.get()).await
}

async fn connect(
    database_url: &str,
    limits: &DatabaseLimits,
    max_connections: u32,
    statement_timeout_ms: u32,
) -> anyhow::Result<PgPool> {
    limits.validate()?;
    let options = PgConnectOptions::from_str(database_url)?.options([
        ("statement_timeout", format!("{statement_timeout_ms}ms")),
        ("lock_timeout", format!("{}ms", limits.lock_timeout_ms)),
        (
            "idle_in_transaction_session_timeout",
            format!("{}ms", limits.idle_transaction_timeout_ms),
        ),
    ]);
    Ok(PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_millis(
            limits.acquire_timeout_ms.get().into(),
        ))
        .connect_with(options)
        .await?)
}
