use super::traits::StorageService;
use crate::{infrastructure::queue::retry_delay, limits::StorageLimits};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone, sqlx::FromRow)]
pub struct CleanupJob {
    pub id: Uuid,
    pub object_url: String,
    pub lease_token: Uuid,
    #[sqlx(try_from = "i32")]
    pub attempts: u32,
}

pub struct StorageCleanup {
    db: PgPool,
    storage: Arc<dyn StorageService>,
    limits: StorageLimits,
}

impl StorageCleanup {
    pub fn new(db: PgPool, storage: Arc<dyn StorageService>, limits: StorageLimits) -> Self {
        Self {
            db,
            storage,
            limits,
        }
    }

    pub async fn start(&self) {
        loop {
            match self.run_once().await {
                Ok(true) => {}
                Ok(false) => {
                    tokio::time::sleep(Duration::from_secs(self.limits.cleanup_poll_seconds)).await
                }
                Err(_) => {
                    tracing::error!("Storage cleanup unavailable; pending work is retained");
                    tokio::time::sleep(Duration::from_secs(self.limits.cleanup_poll_seconds)).await;
                }
            }
        }
    }

    pub async fn claim(&self) -> anyhow::Result<Option<CleanupJob>> {
        Ok(sqlx::query_as(
            "WITH candidate AS (
                SELECT id FROM storage_cleanup
                WHERE (state='ready' AND available_at<=clock_timestamp())
                   OR (state='running' AND lease_expires_at<=clock_timestamp())
                ORDER BY available_at,id FOR UPDATE SKIP LOCKED LIMIT 1
             )
             UPDATE storage_cleanup j SET state='running', attempts=LEAST(j.attempts+1,100),
                lease_token=gen_random_uuid(),
                lease_expires_at=clock_timestamp()+($1::double precision*INTERVAL '1 second')
             FROM candidate c WHERE j.id=c.id
             RETURNING j.id,j.object_url,j.lease_token,j.attempts",
        )
        .bind(self.limits.cleanup_lease_seconds as f64)
        .fetch_optional(&self.db)
        .await?)
    }

    pub async fn run_once(&self) -> anyhow::Result<bool> {
        let Some(job) = self.claim().await? else {
            return Ok(false);
        };
        let Some(key) = self.object_key(&job.object_url) else {
            self.defer(
                &job,
                Some("unrecognized_url"),
                true,
                self.limits.retry_max_seconds,
            )
            .await?;
            tracing::warn!(cleanup_id=%job.id, "Storage cleanup retained an unrecognized object URL for review");
            return Ok(true);
        };
        let referenced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM letterings
             WHERE ARRAY[md5(image_url),md5(thumbnail_small),md5(thumbnail_medium),md5(thumbnail_large)] @> ARRAY[md5($1::text)]
               AND $1=ANY(ARRAY[image_url,thumbnail_small,thumbnail_medium,thumbnail_large]))"
        ).bind(&job.object_url).fetch_one(&self.db).await?;
        if referenced {
            self.defer(&job, None, false, self.limits.retry_max_seconds)
                .await?;
            return Ok(true);
        }
        match tokio::time::timeout(
            Duration::from_secs(self.limits.cleanup_request_timeout_seconds),
            self.storage.delete(&key),
        )
        .await
        {
            Ok(Ok(())) => {
                self.complete(&job).await?;
            }
            _ => {
                let delay = retry_delay(
                    job.attempts,
                    self.limits.retry_initial_seconds,
                    self.limits.retry_max_seconds,
                );
                self.defer(&job, Some("storage_unavailable"), false, delay.as_secs())
                    .await?;
                tracing::warn!(cleanup_id=%job.id, "Object deletion failed; retry retained");
            }
        }
        Ok(true)
    }

    /// A stale worker cannot acknowledge a claim held by a replacement.
    pub async fn complete(&self, job: &CleanupJob) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM storage_cleanup WHERE id=$1 AND lease_token=$2 AND state='running' AND lease_expires_at>clock_timestamp()")
            .bind(job.id).bind(job.lease_token).execute(&self.db).await?.rows_affected() == 1)
    }

    async fn defer(
        &self,
        job: &CleanupJob,
        error: Option<&str>,
        terminal: bool,
        seconds: u64,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE storage_cleanup SET state=$3,last_error_code=$4,
            available_at=clock_timestamp()+($5::double precision*INTERVAL '1 second'),
            lease_token=NULL,lease_expires_at=NULL
            WHERE id=$1 AND lease_token=$2 AND state='running' AND lease_expires_at>clock_timestamp()")
            .bind(job.id).bind(job.lease_token).bind(if terminal {"failed"} else {"ready"})
            .bind(error).bind(seconds as f64).execute(&self.db).await?;
        Ok(())
    }

    // Only this storage provider's immutable application keys may be deleted.
    fn object_key(&self, url: &str) -> Option<String> {
        for prefix in ["letterings/", "thumbs/"] {
            let root = self.storage.get_url(prefix);
            if let Some(filename) = url.strip_prefix(&root)
                && let Some(id) = filename.strip_suffix(".webp")
                && Uuid::parse_str(id).is_ok()
            {
                return Some(format!("{prefix}{filename}"));
            }
        }
        None
    }
}
