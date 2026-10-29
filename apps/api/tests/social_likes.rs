use api::{
    domain::{lettering::errors::DomainError, social::repository::SocialRepository},
    infrastructure::repositories::sqlx_social_repository::SqlxSocialRepository,
};
use sqlx::{PgPool, migrate::Migrator};
use std::{borrow::Cow, sync::Arc};
use tokio::{sync::Barrier, task::JoinSet};
use uuid::Uuid;

async fn account(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'test')")
        .bind(id)
        .bind(format!("{id}@example.test"))
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn fixture(pool: &PgPool) -> (Uuid, Uuid) {
    let owner = account(pool).await;
    let city: Uuid = sqlx::query_scalar("SELECT id FROM cities LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) VALUES($1,$2,$3,'test','image','small','medium','large','560001','APPROVED')")
        .bind(id).bind(city).bind(owner).execute(pool).await.unwrap();
    (id, owner)
}

async fn assert_count(pool: &PgPool, id: Uuid, expected: i32) {
    let (cached, actual): (i32, i32) = sqlx::query_as("SELECT l.likes_count,(SELECT COUNT(*)::int FROM likes WHERE lettering_id=l.id) FROM letterings l WHERE l.id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    assert_eq!((cached, actual), (expected, expected));
}

#[sqlx::test]
async fn accounts_and_anonymous_votes_share_an_address_without_conflicting(pool: PgPool) {
    let (id, first) = fixture(&pool).await;
    let second = account(&pool).await;
    let repo = SqlxSocialRepository::new(pool.clone());
    for (user, expected) in [
        (Some(first), (true, 1)),
        (Some(second), (true, 2)),
        (None, (true, 3)),
        (None, (false, 2)),
        (Some(first), (false, 1)),
    ] {
        assert_eq!(
            repo.toggle_like(id, user, "203.0.113.1").await.unwrap(),
            expected
        );
        assert_count(&pool, id, expected.1).await;
    }
    // The database enforces both identities even if a writer bypasses the API.
    for user in [Some(second), None] {
        if user.is_none() {
            repo.toggle_like(id, None, "203.0.113.1").await.unwrap();
        }
        let error = sqlx::query(
            "INSERT INTO likes(id,lettering_id,user_id,user_ip) VALUES($1,$2,$3,'203.0.113.1')",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23505")
        );
    }
}

async fn compete(pool: &PgPool, id: Uuid, users: Vec<Option<Uuid>>) {
    let barrier = Arc::new(Barrier::new(users.len()));
    let mut tasks = JoinSet::new();
    for user in users {
        let repo = SqlxSocialRepository::new(pool.clone());
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            repo.toggle_like(id, user, "203.0.113.1").await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }
}

#[sqlx::test]
async fn repeated_concurrent_toggles_have_exact_parity(pool: PgPool) {
    let (id, owner) = fixture(&pool).await;
    compete(&pool, id, vec![Some(owner); 24]).await;
    assert_count(&pool, id, 0).await;
    compete(&pool, id, vec![Some(owner); 25]).await;
    assert_count(&pool, id, 1).await;
    compete(&pool, id, vec![None; 24]).await;
    assert_count(&pool, id, 1).await;
    compete(&pool, id, vec![None; 25]).await;
    assert_count(&pool, id, 2).await;
}

#[sqlx::test]
async fn concurrent_accounts_on_one_address_keep_every_vote(pool: PgPool) {
    let (id, _) = fixture(&pool).await;
    let mut users = Vec::new();
    for _ in 0..16 {
        users.push(Some(account(&pool).await));
    }
    compete(&pool, id, users).await;
    assert_count(&pool, id, 16).await;
}

#[sqlx::test]
async fn failed_counter_write_rolls_back_the_vote(pool: PgPool) {
    let (id, owner) = fixture(&pool).await;
    sqlx::query(
        "ALTER TABLE letterings ADD CONSTRAINT fixture_counter_failure CHECK(likes_count=0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        SqlxSocialRepository::new(pool.clone())
            .toggle_like(id, Some(owner), "203.0.113.1")
            .await
            .is_err()
    );
    assert_count(&pool, id, 0).await;
}

#[sqlx::test]
async fn unreadable_uploads_reject_votes_and_deleted_accounts_cannot_vote(pool: PgPool) {
    let (id, owner) = fixture(&pool).await;
    let other = account(&pool).await;
    let repo = SqlxSocialRepository::new(pool.clone());
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    for user in [None, Some(other)] {
        assert!(matches!(
            repo.toggle_like(id, user, "203.0.113.1").await,
            Err(DomainError::NotFound(_))
        ));
    }
    assert_count(&pool, id, 0).await;
    assert_eq!(
        repo.toggle_like(id, Some(owner), "203.0.113.1")
            .await
            .unwrap(),
        (true, 1)
    );
    sqlx::query("UPDATE users SET deleted_at=NOW() WHERE id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        repo.toggle_like(id, Some(owner), "203.0.113.1").await,
        Err(DomainError::Unauthorized)
    ));
    assert_count(&pool, id, 1).await;
}

#[sqlx::test(migrations = false)]
async fn migration_preserves_legacy_votes_and_repairs_counters(pool: PgPool) {
    let mut previous: Migrator = sqlx::migrate!("./migrations");
    previous.migrations = Cow::Owned(
        previous
            .iter()
            .filter(|m| m.version < 20261029000002)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    let (id, owner) = fixture(&pool).await;
    let vote = Uuid::now_v7();
    sqlx::query("INSERT INTO likes(id,lettering_id,user_ip) VALUES($1,$2,'203.0.113.1')")
        .bind(vote)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET likes_count=99 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    assert_count(&pool, id, 1).await;
    let preserved: Uuid = sqlx::query_scalar("SELECT id FROM likes WHERE lettering_id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(preserved, vote);
    assert_eq!(
        SqlxSocialRepository::new(pool.clone())
            .toggle_like(id, Some(owner), "203.0.113.1")
            .await
            .unwrap(),
        (true, 2)
    );
}
