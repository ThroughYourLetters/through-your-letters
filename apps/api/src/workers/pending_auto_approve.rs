use sqlx::{PgPool, Row};
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct PendingAutoApproveWorker {
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
    ) -> Self {
        Self {
            db,
            broadcaster,
            stale_after_minutes: stale_after_minutes.max(1),
            interval_seconds: interval_seconds.max(10),
            batch_size: batch_size.max(1),
        }
    }

    pub async fn start(&self) {
        loop {
            if let Ok(rows) = sqlx::query(
                "WITH stale AS (
                    SELECT id
                    FROM letterings
                    WHERE status = 'PENDING'
                      AND created_at < NOW() - ($1::int * INTERVAL '1 minute')
                    ORDER BY created_at ASC
                    LIMIT $2
                )
                UPDATE letterings
                SET detected_text = COALESCE(detected_text, $3),
                    status = 'APPROVED',
                    updated_at = NOW()
                WHERE id IN (SELECT id FROM stale)
                RETURNING id",
            )
            .bind(self.stale_after_minutes)
            .bind(self.batch_size)
            .bind("Street Discovery")
            .fetch_all(&self.db)
            .await
            {
                for row in rows {
                    if let Ok(id) = row.try_get::<Uuid, _>("id") {
                        let _ = self
                            .broadcaster
                            .send(serde_json::json!({ "type": "PROCESSED", "id": id }).to_string());
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(self.interval_seconds)).await;
        }
    }
}
