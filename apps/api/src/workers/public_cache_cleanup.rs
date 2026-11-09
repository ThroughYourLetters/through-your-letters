use crate::{infrastructure::shutdown::Shutdown, limits::CacheLimits};
use sqlx::PgPool;
use std::{num::NonZeroU32, time::Duration};

pub async fn cleanup_once(db: &PgPool, limit: NonZeroU32) -> anyhow::Result<u64> {
    let mut tx = db.begin().await?;
    let admitted: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('through-your-letters:public-cache-cleanup',0))",
    ).fetch_one(&mut *tx).await?;
    if !admitted {
        tx.commit().await?;
        return Ok(0);
    }
    let removed = sqlx::query(
        "WITH stale AS (SELECT w.backend_pid FROM public_cache_writers w
         WHERE NOT EXISTS(SELECT 1 FROM pg_stat_activity a WHERE a.pid=w.backend_pid)
         ORDER BY w.backend_pid LIMIT $1)
         DELETE FROM public_cache_writers w USING stale WHERE w.backend_pid=stale.backend_pid",
    )
    .bind(i64::from(limit.get()))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if removed > 0 {
        // Rotate the epoch so deleting counters cannot reuse an older cache key.
        let updated =
            sqlx::query("UPDATE public_cache_revision SET epoch=gen_random_uuid() WHERE singleton")
                .execute(&mut *tx)
                .await?
                .rows_affected();
        anyhow::ensure!(updated == 1, "Public cache epoch is missing");
    }
    tx.commit().await?;
    Ok(removed)
}

pub async fn run(db: PgPool, limits: CacheLimits, shutdown: Shutdown) -> anyhow::Result<()> {
    while !shutdown.requested() {
        let cleanup = tokio::time::timeout(
            Duration::from_millis(limits.maintenance_timeout_ms),
            cleanup_once(&db, limits.cleanup_batch_size),
        );
        match shutdown.drain(cleanup).await {
            Some(Ok(Ok(removed))) if removed > 0 => {
                tracing::info!(removed, "Retired inactive public-cache writers")
            }
            Some(Ok(Ok(_))) => {}
            Some(Ok(Err(error))) => {
                let sqlstate = error
                    .downcast_ref::<sqlx::Error>()
                    .and_then(|error| error.as_database_error())
                    .and_then(|error| error.code());
                tracing::warn!(sqlstate = ?sqlstate, "Public-cache writer cleanup failed; retrying at the next interval");
            }
            Some(Err(_)) => {
                tracing::warn!(
                    "Public-cache writer cleanup timed out; retrying at the next interval"
                );
            }
            None => return Ok(()),
        }
        if !shutdown
            .sleep(Duration::from_secs(limits.cleanup_interval_seconds))
            .await
        {
            return Ok(());
        }
    }
    Ok(())
}
