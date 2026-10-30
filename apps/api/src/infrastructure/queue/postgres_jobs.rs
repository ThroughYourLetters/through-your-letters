use super::retry_delay;
use crate::limits::QueueLimits;
use sqlx::{PgConnection, PgPool, postgres::PgListener};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedJob {
    pub lettering_id: Uuid,
    pub image_url: String,
    pub lease_token: Uuid,
    #[sqlx(try_from = "i32")]
    pub attempt: u32,
}

pub enum JobFailure {
    Processing,
    Timeout,
}

impl JobFailure {
    fn code(&self) -> &'static str {
        match self {
            Self::Processing => "processing_failed",
            Self::Timeout => "timed_out",
        }
    }
}

pub struct PostgresJobs {
    pool: PgPool,
    limits: QueueLimits,
}

impl PostgresJobs {
    pub fn new(pool: PgPool, limits: QueueLimits) -> Self {
        Self { pool, limits }
    }

    pub fn limits(&self) -> &QueueLimits {
        &self.limits
    }

    /// Call in the transaction that creates the upload and its ownership.
    pub async fn enqueue_in(
        connection: &mut PgConnection,
        id: Uuid,
        max_attempts: u32,
    ) -> anyhow::Result<()> {
        let inserted = sqlx::query("INSERT INTO analysis_jobs(lettering_id,max_attempts) VALUES($1,$2) ON CONFLICT(lettering_id) DO NOTHING")
            .bind(id).bind(i32::try_from(max_attempts)?).execute(&mut *connection).await?;
        if inserted.rows_affected() > 0 {
            sqlx::query("SELECT pg_notify('ttl_analysis_jobs','')")
                .execute(connection)
                .await?;
        }
        Ok(())
    }

    /// Skip claims held by other workers; an expired lease consumes an attempt.
    pub async fn claim(&self) -> anyhow::Result<Option<ClaimedJob>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("WITH exhausted AS (
            SELECT lettering_id FROM analysis_jobs
            WHERE state='running' AND lease_expires_at<=clock_timestamp() AND attempts>=max_attempts
            ORDER BY lease_expires_at,lettering_id LIMIT $1 FOR UPDATE SKIP LOCKED
        ) UPDATE analysis_jobs SET state='failed',lease_token=NULL,lease_expires_at=NULL,
            last_error_code='lease_expired',finished_at=clock_timestamp(),updated_at=clock_timestamp()
          WHERE lettering_id IN(SELECT lettering_id FROM exhausted)")
            .bind(i64::from(self.limits.recovery_batch_size)).execute(&mut *tx).await?;
        let job = sqlx::query_as::<_,ClaimedJob>("WITH candidate AS (
            SELECT lettering_id FROM analysis_jobs
            WHERE attempts<max_attempts AND (
                (state='ready' AND available_at<=clock_timestamp()) OR
                (state='running' AND lease_expires_at<=clock_timestamp()))
            ORDER BY available_at,lettering_id LIMIT 1 FOR UPDATE SKIP LOCKED
        ) UPDATE analysis_jobs j SET state='running',attempts=j.attempts+1,lease_token=$1,
            lease_expires_at=clock_timestamp()+($2::bigint*INTERVAL '1 second'),updated_at=clock_timestamp(),
            last_error_code=CASE WHEN j.state='running' THEN 'lease_expired' ELSE j.last_error_code END
          FROM candidate c,letterings l WHERE j.lettering_id=c.lettering_id AND l.id=j.lettering_id
          RETURNING j.lettering_id,l.image_url,j.lease_token,j.attempts AS attempt")
            .bind(Uuid::now_v7()).bind(i64::try_from(self.limits.lease_seconds)?)
            .fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(job)
    }

    /// The caller locks the parent upload first and commits its result in this transaction.
    pub async fn acknowledge_in(
        connection: &mut PgConnection,
        job: &ClaimedJob,
    ) -> anyhow::Result<bool> {
        let updated = sqlx::query("UPDATE analysis_jobs SET state='completed',lease_token=NULL,lease_expires_at=NULL,
            last_error_code=NULL,finished_at=clock_timestamp(),updated_at=clock_timestamp()
            WHERE lettering_id=$1 AND state='running' AND lease_token=$2 AND lease_expires_at>clock_timestamp()")
            .bind(job.lettering_id).bind(job.lease_token).execute(connection).await?;
        Ok(updated.rows_affected() == 1)
    }

    pub async fn fail(&self, job: &ClaimedJob, failure: JobFailure) -> anyhow::Result<bool> {
        let delay = retry_delay(
            job.attempt,
            self.limits.retry_initial_seconds,
            self.limits.retry_max_seconds,
        );
        let updated = sqlx::query("UPDATE analysis_jobs SET
            state=CASE WHEN attempts>=max_attempts THEN 'failed' ELSE 'ready' END,
            available_at=clock_timestamp()+($3::bigint*INTERVAL '1 second'),lease_token=NULL,lease_expires_at=NULL,
            last_error_code=$4,finished_at=CASE WHEN attempts>=max_attempts THEN clock_timestamp() END,
            updated_at=clock_timestamp()
            WHERE lettering_id=$1 AND state='running' AND lease_token=$2 AND lease_expires_at>clock_timestamp()")
            .bind(job.lettering_id).bind(job.lease_token).bind(i64::try_from(delay.as_secs())?)
            .bind(failure.code()).execute(&self.pool).await?;
        Ok(updated.rows_affected() == 1)
    }

    /// Reconcile uploads written by old producers during a rolling deployment.
    /// Retained failed or completed jobs never get a fresh retry budget here.
    pub async fn recover_pending(&self) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(
            "WITH missing AS (
            SELECT l.id FROM letterings l WHERE l.status='PENDING' AND l.moderated_at IS NULL
            AND NOT EXISTS(SELECT 1 FROM analysis_jobs j WHERE j.lettering_id=l.id)
            ORDER BY l.created_at,l.id LIMIT $1 FOR UPDATE OF l SKIP LOCKED
        ) INSERT INTO analysis_jobs(lettering_id,max_attempts) SELECT id,$2 FROM missing
          ON CONFLICT(lettering_id) DO NOTHING",
        )
        .bind(i64::from(self.limits.recovery_batch_size))
        .bind(i32::try_from(self.limits.job_max_attempts)?)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() > 0 {
            sqlx::query("SELECT pg_notify('ttl_analysis_jobs','')")
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(inserted.rows_affected())
    }

    pub async fn listener(&self) -> anyhow::Result<PgListener> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen("ttl_analysis_jobs").await?;
        Ok(listener)
    }

    pub async fn next_wake_after(&self) -> anyhow::Result<std::time::Duration> {
        let seconds: f64 = sqlx::query_scalar("SELECT CASE WHEN MIN(wake_at) IS NULL THEN $1::double precision
            ELSE LEAST($1::double precision,GREATEST(EXTRACT(EPOCH FROM (MIN(wake_at)-clock_timestamp()))::double precision,1)) END
            FROM (SELECT MIN(available_at) AS wake_at FROM analysis_jobs WHERE state='ready'
                  UNION ALL SELECT MIN(lease_expires_at) FROM analysis_jobs WHERE state='running') due")
            .bind(self.limits.poll_seconds as f64).fetch_one(&self.pool).await?;
        Ok(std::time::Duration::try_from_secs_f64(seconds)?)
    }
}
