use sqlx::PgPool;
use std::time::Duration;

pub struct AnalyticsWorker {
    db: PgPool,
    poll_seconds: u64,
}
impl AnalyticsWorker {
    pub fn new(db: PgPool, poll_seconds: u64) -> Self {
        Self { db, poll_seconds }
    }
    pub async fn start(&self, shutdown: crate::infrastructure::shutdown::Shutdown) {
        while !shutdown.requested() {
            let update = sqlx::query!(
                "INSERT INTO daily_stats (date, uploads_count) 
                 VALUES (CURRENT_DATE, (SELECT COUNT(*) FROM letterings WHERE created_at::date = CURRENT_DATE)::int)
                 ON CONFLICT (date) DO UPDATE SET uploads_count = EXCLUDED.uploads_count"
            )
            .execute(&self.db);
            match shutdown.drain(update).await {
                Some(Ok(_)) => {}
                Some(Err(_)) => tracing::error!("Analytics update failed"),
                None => return,
            }
            if !shutdown.sleep(Duration::from_secs(self.poll_seconds)).await {
                return;
            }
        }
    }
}
