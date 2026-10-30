use sqlx::PgPool;
use std::time::Duration;

pub struct AnalyticsWorker {
    db: PgPool,
}
impl AnalyticsWorker {
    pub fn new(db: PgPool) -> Self {
        Self { db }
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
            if !shutdown.sleep(Duration::from_secs(3600)).await {
                return;
            }
        }
    }
}
