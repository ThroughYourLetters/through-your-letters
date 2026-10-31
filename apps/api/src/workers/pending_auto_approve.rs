use crate::infrastructure::cache::response_cache::ResponseCache;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct PendingAutoApproveWorker {
    cache: Arc<ResponseCache>,
    db: PgPool,
    broadcaster: Arc<broadcast::Sender<String>>,
    stale_after_minutes: i64,
    interval_seconds: u64,
    batch_size: i64,
}

impl PendingAutoApproveWorker {
    pub fn new(
        db: PgPool,
        broadcaster: Arc<broadcast::Sender<String>>,
        stale_after_minutes: i64,
        interval_seconds: u64,
        batch_size: i64,
        cache: Arc<ResponseCache>,
    ) -> Self {
        Self {
            cache,
            db,
            broadcaster,
            stale_after_minutes: stale_after_minutes.max(1),
            interval_seconds: interval_seconds.max(10),
            batch_size: batch_size.max(1),
        }
    }

    pub async fn start(&self, shutdown: crate::infrastructure::shutdown::Shutdown) {
        while !shutdown.requested() {
            match shutdown.drain(self.run_once()).await {
                Some(Ok(_)) => {}
                Some(Err(_)) => tracing::error!("Pending approval batch failed"),
                None => return,
            }
            if !shutdown
                .sleep(Duration::from_secs(self.interval_seconds))
                .await
            {
                return;
            }
        }
    }

    /// Approve one bounded batch of unreviewed uploads under the opt-in age policy.
    pub async fn run_once(&self) -> Result<usize, sqlx::Error> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            "WITH stale AS (
                    SELECT id
                    FROM letterings
                    WHERE status = 'PENDING'
                      AND moderated_at IS NULL
                      AND created_at < NOW() - ($1::int * INTERVAL '1 minute')
                    ORDER BY created_at ASC, id ASC
                    LIMIT $2
                    FOR UPDATE SKIP LOCKED
                )
                UPDATE letterings
                SET detected_text = COALESCE(detected_text, $3),
                    status = 'APPROVED',
                    updated_at = NOW()
                WHERE id IN (SELECT id FROM stale)
                  AND status = 'PENDING'
                  AND moderated_at IS NULL
                RETURNING id",
        )
        .bind(self.stale_after_minutes)
        .bind(self.batch_size)
        .bind("Street Discovery")
        .fetch_all(&self.db)
        .await?;
        if !ids.is_empty() {
            self.cache.invalidate().await;
        }
        for id in &ids {
            let _ = self
                .broadcaster
                .send(serde_json::json!({ "type": "PROCESSED", "id": id }).to_string());
        }
        Ok(ids.len())
    }
}
