use sqlx::PgPool;
use std::time::Duration;

pub struct AnalyticsWorker {
    db: PgPool,
}
impl AnalyticsWorker {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }
    pub async fn start(&self) {
        loop {
            // Simplified query to ensure no unknown column errors
            let _ = sqlx::query!(
                "INSERT INTO daily_stats (date, uploads_count) 
                 VALUES (CURRENT_DATE, (SELECT COUNT(*) FROM letterings WHERE created_at::date = CURRENT_DATE)::int)
                 ON CONFLICT (date) DO UPDATE SET uploads_count = EXCLUDED.uploads_count"
            ).execute(&self.db).await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }
}
