use super::{
    helpers::{read_json, send},
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

async fn request(
    f: &Fixture,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    send(
        &f.app.app,
        builder
            .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
            .unwrap(),
    )
    .await
}

#[sqlx::test]
async fn owners_can_remove_collection_items_read_notifications_and_delete_uploads(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let path = format!("/api/v1/collections/{}/items/{}", f.collection, f.ids[0]);
    assert_eq!(
        request(&f, "DELETE", &path, None, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&f, "DELETE", &path, Some(&f.token), None)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let collection = f
        .get(&format!("/api/v1/collections/{}", f.collection), false)
        .await;
    assert_eq!(collection["total_items"], 7);
    assert!(
        collection["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["id"] != f.ids[0].to_string())
    );
    let path = format!("/api/v1/me/notifications/{}/read", f.ids[0]);
    assert_eq!(
        request(&f, "POST", &path, Some(&f.token), None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(f.get("/api/v1/me/notifications", false).await["unread"], 7);
    let path = format!("/api/v1/letterings/{}", f.ids[0]);
    assert_eq!(
        request(&f, "DELETE", &path, None, None).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&f, "DELETE", &path, Some(&f.token), None)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(&f, "GET", &path, None, None).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(f.get("/api/v1/me/letterings", false).await["total"], 7);
}

#[sqlx::test]
async fn admin_actions_update_counts_report_partial_success_and_remove_comments(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let protected = request(&f, "GET", "/api/v1/admin/stats", Some(&f.token), None).await;
    assert_eq!(protected.status(), StatusCode::UNAUTHORIZED);
    let stats = f.get("/api/v1/admin/stats", true).await;
    assert_eq!(stats["total_uploads"], 8);
    assert_eq!(stats["approved"], 8);
    sqlx::query("UPDATE letterings SET status='REPORTED',report_count=3,report_reasons='[\"Review\"]'::jsonb WHERE id=$1")
        .bind(f.ids[0]).execute(&pool).await.unwrap();
    let path = format!("/api/v1/admin/letterings/{}/clear-reports", f.ids[0]);
    assert_eq!(
        request(&f, "POST", &path, Some(&f.admin), None)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let moderation = f.get("/api/v1/admin/moderation", true).await;
    let kept = moderation["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == f.ids[0].to_string())
        .unwrap();
    assert_eq!(kept["status"], "APPROVED");
    assert_eq!(kept["report_count"], 0);
    let missing = Uuid::now_v7();
    let response = request(
        &f,
        "POST",
        "/api/v1/admin/letterings/bulk",
        Some(&f.admin),
        Some(json!({"action":"reject","ids":[f.ids[1],missing],"reason":"Review"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value = read_json(response).await;
    assert_eq!(result["processed"], 1);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["failed_items"][0]["id"], missing.to_string());
    let comment = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO comments(id,lettering_id,content) VALUES($1,$2,'Remove this comment')",
    )
    .bind(comment)
    .bind(f.ids[0])
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        request(
            &f,
            "DELETE",
            &format!("/api/v1/admin/comments/{comment}"),
            Some(&f.admin),
            None
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let comments = f
        .get(&format!("/api/v1/letterings/{}/comments", f.ids[0]), false)
        .await;
    assert!(comments.as_array().unwrap().is_empty());
    assert_eq!(
        request(
            &f,
            "DELETE",
            &format!("/api/v1/admin/letterings/{}", f.ids[2]),
            Some(&f.admin),
            None
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let stats = f.get("/api/v1/admin/stats", true).await;
    assert_eq!(stats["total_uploads"], 7);
    assert_eq!(stats["rejected"], 1);
    let response = request(
        &f,
        "POST",
        "/api/v1/admin/cities/discover",
        Some(&f.admin),
        Some(json!({"query":"Empty fixture city","limit":1})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value = read_json(response).await;
    assert_eq!(result, json!({"processed":0,"upserted":0,"failed":0}));
    let docs = f.get("/api/v1/docs", false).await;
    assert_eq!(docs["openapi"], "3.0.0");
    assert!(docs["paths"]["/api/v1/letterings/upload"]["post"].is_object());
}

#[sqlx::test]
async fn verification_tokens_expire_and_cannot_be_reused(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let token = "fixture-verification-token";
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let user: Uuid = sqlx::query_scalar("SELECT user_id FROM letterings WHERE id=$1")
        .bind(f.ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET email_verified=false,email_verification_token_hash=$2,email_verification_expires_at=NOW()-INTERVAL '1 minute' WHERE id=$1").bind(user).bind(hash).execute(&pool).await.unwrap();
    let path = format!("/api/v1/auth/verify-email/{token}");
    assert_eq!(
        request(&f, "GET", "/api/v1/auth/verify-email/invalid", None, None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&f, "GET", &path, None, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    sqlx::query(
        "UPDATE users SET email_verification_expires_at=NOW()+INTERVAL '1 hour' WHERE id=$1",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        request(&f, "GET", &path, None, None).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&f, "GET", &path, None, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT email_verified FROM users WHERE id=$1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
}
