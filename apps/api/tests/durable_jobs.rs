use api::{
    domain::lettering::{
        entity::Lettering,
        repository::{LetteringRepository, NewUpload},
    },
    infrastructure::{
        database::migrations::run_migrations,
        queue::postgres_jobs::{ClaimedJob, JobFailure, PostgresJobs},
        repositories::sqlx_lettering_repository::SqlxLetteringRepository,
    },
};
use sqlx::{PgPool, migrate::Migrator};
use std::{borrow::Cow, collections::HashSet, num::NonZeroU32, sync::Arc, time::Duration};
use uuid::Uuid;

fn queue(pool: &PgPool) -> PostgresJobs {
    PostgresJobs::new(pool.clone(), api::limits::Limits::bundled().unwrap().queue)
}

async fn upload(pool: &PgPool, attempts: Option<u32>) -> Uuid {
    let photo = Lettering {
        id: Uuid::now_v7(),
        city_id: sqlx::query_scalar("SELECT id FROM cities LIMIT 1")
            .fetch_one(pool)
            .await
            .unwrap(),
        contributor_tag: "Queue fixture".into(),
        image_url: "http://fixture.test/image".into(),
        pin_code: "560001".into(),
        ..Default::default()
    };
    SqlxLetteringRepository::new(pool.clone())
        .create(NewUpload {
            request: None,
            lettering: &photo,
            owner_id: None,
            analysis_attempts: attempts.map(|value| NonZeroU32::new(value).unwrap()),
        })
        .await
        .unwrap();
    photo.id
}

async fn expire(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE analysis_jobs SET lease_expires_at=clock_timestamp()-INTERVAL '1 second' WHERE lettering_id=$1")
        .bind(id).execute(pool).await.unwrap();
}

async fn acknowledge(pool: &PgPool, job: &ClaimedJob) -> bool {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM letterings WHERE id=$1 FOR UPDATE")
        .bind(job.lettering_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let applied = PostgresJobs::acknowledge_in(&mut tx, job).await.unwrap();
    tx.commit().await.unwrap();
    applied
}

#[sqlx::test]
async fn competing_workers_claim_each_upload_once(pool: PgPool) {
    let mut expected = HashSet::new();
    for _ in 0..16 {
        expected.insert(upload(&pool, Some(3)).await);
    }
    let queue = Arc::new(queue(&pool));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let queue = queue.clone();
        tasks.spawn(async move { queue.claim().await.unwrap() });
    }
    let mut actual = HashSet::new();
    while let Some(result) = tasks.join_next().await {
        if let Some(job) = result.unwrap() {
            assert_eq!(job.attempt, 1);
            assert!(actual.insert(job.lettering_id));
        }
    }
    assert_eq!(actual, expected);
    let running: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analysis_jobs WHERE state='running' AND attempts=1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(running, 16);
}

#[sqlx::test]
async fn idle_and_delayed_wakes_use_the_configured_deadlines(pool: PgPool) {
    let mut limits = api::limits::Limits::bundled().unwrap().queue;
    limits.poll_seconds = 60;
    let queue = PostgresJobs::new(pool.clone(), limits);
    assert_eq!(
        queue.next_wake_after().await.unwrap(),
        Duration::from_secs(60)
    );
    upload(&pool, Some(3)).await;
    let job = queue.claim().await.unwrap().unwrap();
    queue.fail(&job, JobFailure::Processing).await.unwrap();
    let wait = queue.next_wake_after().await.unwrap();
    assert!(wait >= Duration::from_secs(1) && wait <= Duration::from_secs(5));
    assert!(queue.claim().await.unwrap().is_none());
}

#[sqlx::test]
async fn concurrent_account_deletion_cannot_leave_a_new_owned_upload_or_job(pool: PgPool) {
    let owner = Uuid::now_v7();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,'deleted-owner@example.test','fixture')")
        .bind(owner).execute(&pool).await.unwrap();
    let photo = Lettering {
        id: Uuid::now_v7(),
        city_id: sqlx::query_scalar("SELECT id FROM cities LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap(),
        contributor_tag: "Deleted owner".into(),
        image_url: "http://fixture.test/image".into(),
        pin_code: "560001".into(),
        ..Default::default()
    };
    let mut deletion = pool.begin().await.unwrap();
    sqlx::query("UPDATE users SET deleted_at=NOW() WHERE id=$1")
        .bind(owner)
        .execute(&mut *deletion)
        .await
        .unwrap();
    let repo = SqlxLetteringRepository::new(pool.clone());
    let creation = tokio::spawn(async move {
        repo.create(NewUpload {
            request: None,
            lettering: &photo,
            owner_id: Some(owner),
            analysis_attempts: NonZeroU32::new(3),
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND state='active' AND wait_event_type='Lock')")
                .fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    deletion.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), creation)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(api::domain::lettering::errors::DomainError::Unauthorized)
    ));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM letterings),(SELECT COUNT(*) FROM analysis_jobs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
}

#[sqlx::test]
async fn abandoned_leases_survive_restart_and_exhaust_a_finite_attempt_budget(pool: PgPool) {
    let id = upload(&pool, Some(3)).await;
    let first = queue(&pool).claim().await.unwrap().unwrap();
    expire(&pool, id).await;
    let restarted = queue(&pool);
    let second = restarted.claim().await.unwrap().unwrap();
    assert_eq!(second.attempt, 2);
    assert_ne!(first.lease_token, second.lease_token);
    assert!(!acknowledge(&pool, &first).await);
    assert!(
        !restarted
            .fail(&first, JobFailure::Processing)
            .await
            .unwrap()
    );
    expire(&pool, id).await;
    let third = restarted.claim().await.unwrap().unwrap();
    assert_eq!(third.attempt, 3);
    expire(&pool, id).await;
    assert!(restarted.claim().await.unwrap().is_none());
    let failed:(String,i32,String,bool)=sqlx::query_as("SELECT state,attempts,last_error_code,finished_at IS NOT NULL FROM analysis_jobs WHERE lettering_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(failed, ("failed".into(), 3, "lease_expired".into(), true));
    assert_eq!(restarted.recover_pending().await.unwrap(), 0);
    assert!(!acknowledge(&pool, &third).await);
}

#[sqlx::test]
async fn retries_wait_and_keep_permanent_failures_without_resetting_their_budget(pool: PgPool) {
    let id = upload(&pool, Some(3)).await;
    let mut limits = api::limits::Limits::bundled().unwrap().queue;
    limits.retry_initial_seconds = 60;
    let queue = PostgresJobs::new(pool.clone(), limits);
    for attempt in 1..=3 {
        let job = queue.claim().await.unwrap().unwrap();
        assert_eq!(job.attempt, attempt);
        assert!(queue.fail(&job, JobFailure::Processing).await.unwrap());
        assert!(!queue.fail(&job, JobFailure::Timeout).await.unwrap());
        assert!(queue.claim().await.unwrap().is_none());
        let row:(String,i32,bool,bool)=sqlx::query_as("SELECT state,attempts,available_at>clock_timestamp(),lease_token IS NULL AND lease_expires_at IS NULL FROM analysis_jobs WHERE lettering_id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            row,
            (
                if attempt == 3 {
                    "failed".into()
                } else {
                    "ready".into()
                },
                attempt as i32,
                true,
                true
            )
        );
        sqlx::query("UPDATE analysis_jobs SET available_at='-infinity' WHERE lettering_id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(queue.recover_pending().await.unwrap(), 0);
    assert!(queue.claim().await.unwrap().is_none());
}

#[sqlx::test]
async fn notifications_and_job_rows_become_visible_only_after_commit(pool: PgPool) {
    let id = upload(&pool, None).await;
    let queue = queue(&pool);
    let mut listener = queue.listener().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    PostgresJobs::enqueue_in(&mut tx, id, 3).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.recv())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(queue.claim().await.unwrap().is_none());
    let mut tx = pool.begin().await.unwrap();
    PostgresJobs::enqueue_in(&mut tx, id, 3).await.unwrap();
    tx.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), listener.recv())
        .await
        .unwrap()
        .unwrap();
    let job = queue.claim().await.unwrap().unwrap();
    assert_eq!(job.lettering_id, id);
    assert!(acknowledge(&pool, &job).await);
    assert!(!acknowledge(&pool, &job).await);
}

#[sqlx::test]
async fn rolling_deployment_recovery_is_bounded_idempotent_and_preserves_reviews(pool: PgPool) {
    let mut expected = HashSet::new();
    for _ in 0..3 {
        expected.insert(upload(&pool, None).await);
    }
    for (status, reviewed) in [("REJECTED", false), ("APPROVED", false), ("PENDING", true)] {
        let id = upload(&pool, None).await;
        sqlx::query(
            "UPDATE letterings SET status=$2,moderated_at=CASE WHEN $3 THEN NOW() END WHERE id=$1",
        )
        .bind(id)
        .bind(status)
        .bind(reviewed)
        .execute(&pool)
        .await
        .unwrap();
    }
    let mut limits = api::limits::Limits::bundled().unwrap().queue;
    limits.recovery_batch_size = 2;
    let queue = PostgresJobs::new(pool.clone(), limits);
    assert_eq!(queue.recover_pending().await.unwrap(), 2);
    assert_eq!(queue.recover_pending().await.unwrap(), 1);
    assert_eq!(queue.recover_pending().await.unwrap(), 0);
    let actual: Vec<Uuid> = sqlx::query_scalar("SELECT lettering_id FROM analysis_jobs")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(actual.into_iter().collect::<HashSet<_>>(), expected);
}

#[sqlx::test(migrations = false)]
async fn migration_recovers_legacy_unreviewed_uploads_without_changing_decisions(pool: PgPool) {
    let mut previous: Migrator = sqlx::migrate!("./migrations");
    previous.migrations = Cow::Owned(
        previous
            .iter()
            .filter(|migration| migration.version < 20261030000002)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    let pending = upload(&pool, None).await;
    let rejected = upload(&pool, None).await;
    sqlx::query("UPDATE letterings SET status='REJECTED',moderation_reason='Keep hidden',moderated_at=NOW() WHERE id=$1")
        .bind(rejected).execute(&pool).await.unwrap();
    run_migrations(&pool, false).await.unwrap();
    run_migrations(&pool, false).await.unwrap();
    let rows: Vec<(Uuid, i32, String)> =
        sqlx::query_as("SELECT lettering_id,attempts,state FROM analysis_jobs")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows, vec![(pending, 0, "ready".into())]);
    let decision: String = sqlx::query_scalar("SELECT status FROM letterings WHERE id=$1")
        .bind(rejected)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(decision, "REJECTED");
}
