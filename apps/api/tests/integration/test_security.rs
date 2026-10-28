use super::helpers::{TestApp, expect_status, read_json, send, spawn_app, unique_email};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn assert_public_json(value: &Value) {
    match value {
        Value::Object(fields) => {
            assert!(!fields.contains_key("uploaded_by_ip"));
            assert!(!fields.contains_key("report_reasons"));
            for child in fields.values() {
                assert_public_json(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_public_json(item);
            }
        }
        _ => {}
    }
}

fn request(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}
async fn account(app: &TestApp) -> (String, Uuid) {
    let response = send(
        &app.app,
        request(
            "POST",
            "/api/v1/auth/register",
            None,
            json!({"email":unique_email("security"),"password":"SecurePassword123!"}),
        ),
    )
    .await;
    let value: Value = read_json(expect_status(response, StatusCode::OK).await).await;
    (
        value["token"].as_str().unwrap().into(),
        Uuid::parse_str(value["user"]["id"].as_str().unwrap()).unwrap(),
    )
}
async fn lettering(app: &TestApp, owner: Uuid, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status,user_id,uploaded_by_ip,report_reasons,detected_text) VALUES($1,'0194f123-4567-7abc-8def-0123456789ab','Tester','https://example.test/image','small','medium','large',ST_GeogFromText('POINT(77 12)'),'560001',$2,$3,'203.0.113.1','[\"private report\"]','Security fixture')")
        .bind(id).bind(status).bind(owner).execute(&app.db).await.unwrap();
    id
}
#[tokio::test]
async fn public_responses_exclude_private_fields_and_hidden_records() {
    let app = spawn_app().await;
    let (token, owner) = account(&app).await;
    let visible = lettering(&app, owner, "APPROVED").await;
    let hidden = lettering(&app, owner, "REJECTED").await;
    let response = send(
        &app.app,
        request(
            "GET",
            &format!("/api/v1/letterings/{visible}"),
            None,
            Value::Null,
        ),
    )
    .await;
    let value: Value = read_json(expect_status(response, StatusCode::OK).await).await;
    assert!(value.get("uploaded_by_ip").is_none());
    assert!(value.get("report_reasons").is_none());
    for path in [
        "/api/v1/letterings",
        "/api/v1/letterings/search?q=Security",
        "/api/v1/contributors/Tester",
    ] {
        let response = send(&app.app, request("GET", path, None, Value::Null)).await;
        let body: Value = read_json(expect_status(response, StatusCode::OK).await).await;
        assert_public_json(&body);
        assert!(body.to_string().contains(&visible.to_string()));
    }

    for suffix in ["", "/download", "/revisits", "/similar"] {
        let response = send(
            &app.app,
            request(
                "GET",
                &format!("/api/v1/letterings/{hidden}{suffix}"),
                None,
                Value::Null,
            ),
        )
        .await;
        expect_status(response, StatusCode::NOT_FOUND).await;
    }
    let response = send(
        &app.app,
        request(
            "GET",
            &format!("/api/v1/letterings/{hidden}"),
            Some(&token),
            Value::Null,
        ),
    )
    .await;
    expect_status(response, StatusCode::OK).await;
    let encoded = hidden.to_string().replace('-', "%2D");
    expect_status(
        send(
            &app.app,
            request(
                "GET",
                &format!("/api/v1/letterings/{encoded}"),
                None,
                Value::Null,
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let (other, _) = account(&app).await;
    let response = send(
        &app.app,
        request(
            "GET",
            &format!("/api/v1/letterings/{hidden}"),
            Some(&other),
            Value::Null,
        ),
    )
    .await;
    expect_status(response, StatusCode::NOT_FOUND).await;
}
#[tokio::test]
async fn concurrent_reset_is_single_use_and_revokes_old_sessions() {
    let app = spawn_app().await;
    let (token, user) = account(&app).await;
    let raw = Uuid::now_v7().to_string();
    sqlx::query("INSERT INTO password_reset_tokens(id,user_id,token_hash,expires_at) VALUES($1,$2,$3,NOW()+INTERVAL '1 hour')")
        .bind(Uuid::now_v7()).bind(user).bind(format!("{:x}", Sha256::digest(raw.as_bytes()))).execute(&app.db).await.unwrap();
    let body = json!({"token":raw,"new_password":"ReplacementPassword123!"});
    let (first, second) = tokio::join!(
        send(
            &app.app,
            request("POST", "/api/v1/auth/reset-password", None, body.clone())
        ),
        send(
            &app.app,
            request("POST", "/api/v1/auth/reset-password", None, body)
        )
    );
    let mut statuses = vec![first.status().as_u16(), second.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 400]);
    let response = send(
        &app.app,
        request("GET", "/api/v1/auth/me", Some(&token), Value::Null),
    )
    .await;
    expect_status(response, StatusCode::UNAUTHORIZED).await;
}
#[tokio::test]
async fn private_collections_require_the_owner() {
    let app = spawn_app().await;
    let (token, owner) = account(&app).await;
    let (other, _) = account(&app).await;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO collections(id,name,creator_tag,is_public,owner_user_id) VALUES($1,'Private','Tester',false,$2)")
        .bind(id).bind(owner).execute(&app.db).await.unwrap();
    for (token, status) in [
        (None, StatusCode::NOT_FOUND),
        (Some(other.as_str()), StatusCode::NOT_FOUND),
        (Some(token.as_str()), StatusCode::OK),
    ] {
        let response = send(
            &app.app,
            request(
                "GET",
                &format!("/api/v1/collections/{id}"),
                token,
                Value::Null,
            ),
        )
        .await;
        expect_status(response, status).await;
    }
}
#[tokio::test]
async fn deletion_anonymizes_contributions_and_revokes_session() {
    let app = spawn_app().await;
    let (token, owner) = account(&app).await;
    let id = lettering(&app, owner, "APPROVED").await;
    let private_collection = Uuid::now_v7();
    let public_collection = Uuid::now_v7();
    for (collection, public) in [(private_collection, false), (public_collection, true)] {
        sqlx::query("INSERT INTO collections(id,name,creator_tag,is_public,owner_user_id) VALUES($1,'Fixture','Personal name',$2,$3)")
            .bind(collection).bind(public).bind(owner).execute(&app.db).await.unwrap();
    }
    sqlx::query("INSERT INTO comments(id,lettering_id,user_id,content,user_ip) VALUES($1,$2,$3,'Personal text','203.0.113.1')")
        .bind(Uuid::now_v7()).bind(id).bind(owner).execute(&app.db).await.unwrap();
    let response = send(
        &app.app,
        request(
            "DELETE",
            "/api/v1/me/account",
            Some(&token),
            json!({"password":"SecurePassword123!"}),
        ),
    )
    .await;
    expect_status(response, StatusCode::NO_CONTENT).await;
    let comments: (
        Option<Uuid>,
        String,
        Option<sqlx::types::ipnetwork::IpNetwork>,
    ) = sqlx::query_as("SELECT user_id,content,user_ip FROM comments WHERE lettering_id=$1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(comments, (None, "[deleted]".into(), None));
    let retained: Vec<(Uuid, Option<Uuid>, String)> =
        sqlx::query_as("SELECT id,owner_user_id,creator_tag FROM collections WHERE id=ANY($1)")
            .bind(vec![private_collection, public_collection])
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(
        retained,
        vec![(public_collection, None, "Anonymous".into())]
    );
    let row: (
        Option<Uuid>,
        String,
        Option<sqlx::types::ipnetwork::IpNetwork>,
    ) = sqlx::query_as("SELECT user_id,contributor_tag,uploaded_by_ip FROM letterings WHERE id=$1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(row, (None, "Anonymous".into(), None));
    expect_status(
        send(
            &app.app,
            request("GET", "/api/v1/auth/me", Some(&token), Value::Null),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[tokio::test]
async fn missing_invalid_and_expired_sessions_return_unauthorized() {
    let app = spawn_app().await;
    let (_, id) = account(&app).await;
    let claims = api::presentation::http::middleware::user::UserClaims {
        sub: id.to_string(),
        email: "fixture@example.com".into(),
        role: "USER".into(),
        exp: 1,
        session_version: 0,
    };
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    for token in [None, Some("invalid"), Some(expired.as_str())] {
        expect_status(
            send(
                &app.app,
                request("GET", "/api/v1/auth/me", token, Value::Null),
            )
            .await,
            StatusCode::UNAUTHORIZED,
        )
        .await;
    }
}
