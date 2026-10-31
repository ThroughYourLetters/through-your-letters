use crate::{
    domain::lettering::{entity::Lettering, repository::UploadAttempt},
    infrastructure::queue::retry_delay,
    limits::StorageLimits,
};
use sqlx::{PgConnection, PgPool};
use std::time::Duration;
use uuid::Uuid;

pub struct PostgresUploadAttempts {
    db: PgPool,
    limits: StorageLimits,
}

#[derive(sqlx::FromRow)]
struct AbandonedAttempt {
    id: Uuid,
    image_url: String,
    thumbnail_url: String,
    #[sqlx(try_from = "i32")]
    cleanup_rounds: u32,
}

impl PostgresUploadAttempts {
    pub fn new(db: PgPool, limits: StorageLimits) -> Self {
        Self { db, limits }
    }

    /// Persist both keys before any storage request can create an object.
    pub async fn begin(
        &self,
        id: Uuid,
        image_url: &str,
        thumbnail_url: &str,
    ) -> anyhow::Result<UploadAttempt> {
        let lease_token = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO upload_attempts(id,lease_token,lease_expires_at,image_url,thumbnail_url)
            VALUES($1,$2,clock_timestamp()+($3::double precision*INTERVAL '1 second'),$4,$5)",
        )
        .bind(id)
        .bind(lease_token)
        .bind(self.limits.upload_lease_seconds as f64)
        .bind(image_url)
        .bind(thumbnail_url)
        .execute(&self.db)
        .await?;
        Ok(UploadAttempt { id, lease_token })
    }

    /// The caller holds this row lock until the source transaction resolves.
    pub async fn lock_in(
        connection: &mut PgConnection,
        attempt: UploadAttempt,
        lettering: &Lettering,
    ) -> Result<bool, sqlx::Error> {
        if attempt.id != lettering.id {
            return Ok(false);
        }
        let row: Option<(String, String)> = sqlx::query_as("SELECT image_url,thumbnail_url FROM upload_attempts
            WHERE id=$1 AND lease_token=$2 AND state='uploading' AND lease_expires_at>clock_timestamp() FOR UPDATE")
            .bind(attempt.id).bind(attempt.lease_token).fetch_optional(connection).await?;
        Ok(row.is_some_and(|(image, thumbnail)| {
            image == lettering.image_url
                && image == lettering.thumbnail_urls.large
                && thumbnail == lettering.thumbnail_urls.small
                && thumbnail == lettering.thumbnail_urls.medium
        }))
    }

    pub async fn commit_in(
        connection: &mut PgConnection,
        attempt: UploadAttempt,
    ) -> Result<bool, sqlx::Error> {
        // lock_in already excludes reconciliation; COMMIT may complete after the HTTP caller leaves.
        Ok(sqlx::query(
            "UPDATE upload_attempts SET state='committed',lettering_id=$1
            WHERE id=$1 AND lease_token=$2 AND state='uploading'",
        )
        .bind(attempt.id)
        .bind(attempt.lease_token)
        .execute(connection)
        .await?
        .rows_affected()
            == 1)
    }

    pub async fn start(&self, shutdown: crate::infrastructure::shutdown::Shutdown) {
        while !shutdown.requested() {
            match shutdown.drain(self.reconcile_once()).await {
                Some(Ok(_)) => {}
                Some(Err(_)) => {
                    tracing::error!("Upload reconciliation unavailable; attempts remain durable")
                }
                None => return,
            }
            if !shutdown
                .sleep(Duration::from_secs(self.limits.orphan_scan_seconds))
                .await
            {
                return;
            }
        }
    }

    pub async fn reconcile_once(&self) -> anyhow::Result<usize> {
        let mut tx = self.db.begin().await?;
        let attempts: Vec<AbandonedAttempt> = sqlx::query_as(
            "SELECT id,image_url,thumbnail_url,cleanup_rounds FROM upload_attempts
            WHERE (state='uploading' AND lease_expires_at<=clock_timestamp())
               OR (state='cleanup' AND next_cleanup_at<=clock_timestamp())
            ORDER BY lease_expires_at,id FOR UPDATE SKIP LOCKED LIMIT $1",
        )
        .bind(i64::from(self.limits.orphan_batch_size))
        .fetch_all(&mut *tx)
        .await?;
        for attempt in &attempts {
            sqlx::query("INSERT INTO storage_cleanup(object_url) SELECT DISTINCT url FROM unnest(ARRAY[$1::text,$2::text]) AS objects(url)
                ON CONFLICT(object_url) DO UPDATE SET available_at=LEAST(storage_cleanup.available_at,clock_timestamp())")
                .bind(&attempt.image_url).bind(&attempt.thumbnail_url).execute(&mut *tx).await?;
            let round = attempt.cleanup_rounds.saturating_add(1).min(100);
            let delay = retry_delay(
                round,
                self.limits.orphan_retry_initial_seconds,
                self.limits.orphan_retry_max_seconds,
            );
            sqlx::query("UPDATE upload_attempts SET state='cleanup',cleanup_rounds=$2,
                next_cleanup_at=clock_timestamp()+($3::double precision*INTERVAL '1 second') WHERE id=$1")
                .bind(attempt.id).bind(round as i32).bind(delay.as_secs() as f64).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(attempts.len())
    }
}
