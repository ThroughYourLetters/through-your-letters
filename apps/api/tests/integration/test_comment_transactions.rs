use super::helpers::{TestApp, read_json, send, spawn_app_with_config};
use api::{
    domain::{
        lettering::errors::DomainError,
        social::{comment::CommentModerationInput, repository::SocialRepository},
    },
    infrastructure::repositories::sqlx_social_repository::SqlxSocialRepository,
    presentation::http::middleware::{admin::AdminClaims, user::UserClaims},
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::{sync::Barrier, task::JoinSet};
use uuid::Uuid;

fn assessment(visible: bool) -> CommentModerationInput {
    CommentModerationInput {
        status: if visible { "VISIBLE" } else { "HIDDEN" }.into(),
        moderation_score: 0,
        moderation_flags: vec![],
        auto_flagged: !visible,
        needs_review: !visible,
        review_priority: 0,
        moderated_by: None,
        moderation_reason: None,
    }
}

struct Fixture {
    app: TestApp,
    user: Uuid,
    token: String,
    photo: Uuid,
    admin: String,
}

impl Fixture {
    async fn new(pool: &PgPool, redis_unavailable: bool) -> Self {
        let app = spawn_app_with_config(pool, |config| {
            config.limits.security.comment.requests = 1;
            if redis_unavailable {
                config.redis_url = "redis://127.0.0.1:9".into();
                config.limits.security.redis_connect_timeout_ms = 100;
                config.limits.security.redis_response_timeout_ms = 100;
            }
        })
        .await;
        let user = Uuid::now_v7();
        let email = format!("{user}@example.test");
        sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'test')")
            .bind(user)
            .bind(&email)
            .execute(pool)
            .await
            .unwrap();
        let city: Uuid = sqlx::query_scalar("SELECT id FROM cities LIMIT 1")
            .fetch_one(pool)
            .await
            .unwrap();
        let photo = Uuid::now_v7();
        sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) VALUES($1,$2,$3,'test','image','small','medium','large','560001','APPROVED')")
            .bind(photo).bind(city).bind(user).execute(pool).await.unwrap();
        let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;
        let token = encode(
            &Header::default(),
            &UserClaims {
                sub: user.to_string(),
                email,
                role: "USER".into(),
                exp,
                session_version: 0,
            },
            &EncodingKey::from_secret(b"test-jwt-secret"),
        )
        .unwrap();
        let admin = encode(
            &Header::default(),
            &AdminClaims {
                sub: "comment-admin".into(),
                exp,
                aud: "admin".into(),
            },
            &EncodingKey::from_secret(b"test-admin-jwt-secret"),
        )
        .unwrap();
        Self {
            app,
            user,
            token,
            photo,
            admin,
        }
    }
    fn comment_request(&self) -> Request<Body> {
        request(
            "POST",
            &format!("/api/v1/letterings/{}/comments", self.photo),
            &self.token,
            json!({"content":"Beautiful lettering, thank you for sharing"}),
        )
    }
    async fn comment(&self) -> Uuid {
        SqlxSocialRepository::new(self.app.db.clone())
            .add_comment(
                self.photo,
                self.user,
                "A public comment".into(),
                Some("203.0.113.1"),
                assessment(true),
            )
            .await
            .unwrap()
            .id
    }
}

fn request(method: &str, path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn assert_count(pool: &PgPool, photo: Uuid, expected: i32) {
    let counts: (i32,i32) = sqlx::query_as("SELECT comments_count,(SELECT COUNT(*)::int FROM comments WHERE lettering_id=l.id AND status='VISIBLE') FROM letterings l WHERE id=$1")
        .bind(photo).fetch_one(pool).await.unwrap();
    assert_eq!(counts, (expected, expected));
}

#[sqlx::test]
async fn comment_throttle_reserves_only_one_concurrent_slot(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    let mut tasks = JoinSet::new();
    let barrier = Arc::new(Barrier::new(24));
    for _ in 0..24 {
        let app = f.app.app.clone();
        let req = f.comment_request();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            send(&app, req).await.status()
        });
    }
    let mut created = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            StatusCode::OK => created += 1,
            StatusCode::TOO_MANY_REQUESTS => {}
            status => panic!("unexpected {status}"),
        }
    }
    assert_eq!(created, 1);
    assert_count(&pool, f.photo, 1).await;
}

#[sqlx::test]
async fn unavailable_throttle_backend_rejects_without_writes(pool: PgPool) {
    let f = Fixture::new(&pool, true).await;
    assert_eq!(
        send(&f.app.app, f.comment_request()).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_count(&pool, f.photo, 0).await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn failed_comment_counter_and_unreadable_parent_leave_no_comment(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    sqlx::query(
        "ALTER TABLE letterings ADD CONSTRAINT fixture_no_comments CHECK(comments_count=0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let repo = SqlxSocialRepository::new(pool.clone());
    assert!(
        repo.add_comment(
            f.photo,
            f.user,
            "test".into(),
            Some("203.0.113.1"),
            assessment(true)
        )
        .await
        .is_err()
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_count(&pool, f.photo, 0).await;
    sqlx::query("UPDATE letterings SET user_id=NULL,status='PENDING' WHERE id=$1")
        .bind(f.photo)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        repo.add_comment(f.photo, f.user, "test".into(), None, assessment(true))
            .await,
        Err(DomainError::NotFound(_))
    ));
    assert_eq!(
        send(&f.app.app, f.comment_request()).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn concurrent_comment_inserts_count_only_visible_rows(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    let mut tasks = JoinSet::new();
    for index in 0..24 {
        let repo = SqlxSocialRepository::new(pool.clone());
        let photo = f.photo;
        let user = f.user;
        tasks.spawn(async move {
            repo.add_comment(photo, user, "test".into(), None, assessment(index % 2 == 0))
                .await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }
    assert_count(&pool, f.photo, 12).await;
}

#[sqlx::test]
async fn counter_or_audit_failure_rolls_back_moderation_and_notifications(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    let id = f.comment().await;
    sqlx::query(
        "ALTER TABLE letterings ADD CONSTRAINT fixture_keep_counter CHECK(comments_count=1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    for step in 0..2 {
        let response = send(
            &f.app.app,
            request(
                "POST",
                &format!("/api/v1/admin/comments/{id}/hide"),
                &f.admin,
                json!({"reason":"Review"}),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let status: String = sqlx::query_scalar("SELECT status FROM comments WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "VISIBLE");
        assert_count(&pool, f.photo, 1).await;
        let audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        let notices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((audit, notices), (0, 0));
        if step == 0 {
            sqlx::query("ALTER TABLE letterings DROP CONSTRAINT fixture_keep_counter")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "ALTER TABLE admin_audit_logs ADD CONSTRAINT fixture_audit_failure CHECK(false)",
            )
            .execute(&pool)
            .await
            .unwrap();
        }
    }
}

#[sqlx::test]
async fn bulk_moderation_commits_successes_and_rolls_back_each_failed_item(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    let good = f.comment().await;
    let bad = f.comment().await;
    sqlx::query(&format!("ALTER TABLE admin_audit_logs ADD CONSTRAINT fixture_audit_failure CHECK(metadata->>'comment_id' IS DISTINCT FROM '{bad}')")).execute(&pool).await.unwrap();
    let response = send(
        &f.app.app,
        request(
            "POST",
            "/api/v1/admin/comments/bulk",
            &f.admin,
            json!({"ids":[good,bad,Uuid::now_v7()],"action":"hide","reason":"Review"}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = read_json(response).await;
    assert_eq!(body["processed"], 1);
    assert_eq!(body["failed"], 2);
    assert!(!body.to_string().contains("fixture_audit_failure"));
    let statuses: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id,status FROM comments ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        statuses,
        vec![(good, "HIDDEN".into()), (bad, "VISIBLE".into())]
    );
    assert_count(&pool, f.photo, 1).await;
    let audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    let notices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((audit, notices), (1, 1));
}

#[sqlx::test]
async fn competing_moderation_actions_preserve_counts_and_audit_history(pool: PgPool) {
    let f = Fixture::new(&pool, false).await;
    let id = f.comment().await;
    let mut tasks = JoinSet::new();
    for index in 0..24 {
        let app = f.app.app.clone();
        let action = if index % 2 == 0 { "hide" } else { "restore" };
        let req = request(
            "POST",
            &format!("/api/v1/admin/comments/{id}/{action}"),
            &f.admin,
            json!({"reason":"Review"}),
        );
        tasks.spawn(async move { send(&app, req).await.status() });
    }
    while let Some(result) = tasks.join_next().await {
        assert_eq!(result.unwrap(), StatusCode::OK);
    }
    let actual: i32 =
        sqlx::query_scalar("SELECT COUNT(*)::int FROM comments WHERE status='VISIBLE'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_count(&pool, f.photo, actual).await;
    let audit: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_logs WHERE lettering_id=$1")
            .bind(f.photo)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit, 24);
}

#[sqlx::test]
async fn saving_metadata_cannot_overwrite_social_counters_from_a_stale_entity(pool: PgPool) {
    use api::{
        domain::lettering::repository::LetteringRepository,
        infrastructure::repositories::sqlx_lettering_repository::SqlxLetteringRepository,
    };
    let f = Fixture::new(&pool, false).await;
    let repository = SqlxLetteringRepository::new(pool.clone());
    let mut stale = repository.find_by_id(f.photo).await.unwrap().unwrap();
    f.comment().await;
    SqlxSocialRepository::new(pool.clone())
        .toggle_like(f.photo, Some(f.user), "203.0.113.1")
        .await
        .unwrap();
    stale.description = Some("Updated description".into());
    let saved = repository.update(&stale).await.unwrap();
    assert_eq!((saved.likes_count, saved.comments_count), (1, 1));
    assert_count(&pool, f.photo, 1).await;
}
