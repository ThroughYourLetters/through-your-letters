use super::helpers::{
    TestApp, expect_status, multipart_upload_body, read_json, send, spawn_app,
    spawn_app_with_config, tiny_png_bytes, unique_email,
};
use api::presentation::http::middleware::user::UserClaims;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use uuid::Uuid;

fn request(path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn account_and_upload(app: &TestApp) -> (String, Uuid, Uuid, String) {
    let user = Uuid::now_v7();
    let email = unique_email("security-boundary");
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,$2,'unused','USER')")
        .bind(user)
        .bind(&email)
        .execute(&app.db)
        .await
        .unwrap();
    let claims = UserClaims {
        sub: user.to_string(),
        email: email.clone(),
        role: "USER".into(),
        session_version: 0,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status) VALUES($1,'0194f123-4567-7abc-8def-0123456789ab',$2,'Security boundary','image','small','medium','large',NULL,'560025','APPROVED')")
        .bind(id).bind(user).execute(&app.db).await.unwrap();
    (token, user, id, email)
}

#[tokio::test]
async fn redis_outages_block_protected_actions_without_blocking_public_reads() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = format!("redis://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = spawn_app_with_config(|config| {
        config.redis_url = unavailable;
        config.limits.security.redis_connect_timeout_ms = 100;
        config.limits.security.redis_response_timeout_ms = 100;
    })
    .await;
    let (token, user, id, email) = account_and_upload(&app).await;
    let registration_email = unique_email("blocked-register");
    let cases = [
        (
            "/api/v1/auth/register".to_string(),
            None,
            json!({"email":registration_email,"password":"SecurePassword123!"}),
        ),
        (
            "/api/v1/auth/login".into(),
            None,
            json!({"email":email,"password":"SecurePassword123!"}),
        ),
        (
            "/api/v1/auth/forgot-password".into(),
            None,
            json!({"email":email}),
        ),
        (
            "/api/v1/auth/reset-password".into(),
            None,
            json!({"token":"not-a-token","new_password":"SecurePassword123!"}),
        ),
        (
            "/api/v1/admin/login".into(),
            None,
            json!({"email":app.admin_email,"password":app.admin_password}),
        ),
        (
            format!("/api/v1/letterings/{id}/comments"),
            Some(token.as_str()),
            json!({"content":"Wonderful lettering"}),
        ),
        (
            format!("/api/v1/letterings/{id}/report"),
            Some(token.as_str()),
            json!({"reason":"Review this upload"}),
        ),
    ];
    for (path, token, body) in cases {
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            send(&app.app, request(&path, token, body)),
        )
        .await
        .unwrap();
        let body: Value =
            read_json(expect_status(response, StatusCode::SERVICE_UNAVAILABLE).await).await;
        assert_eq!(body, json!({"error":"External service unavailable"}));
    }
    let (boundary, body) = multipart_upload_body(
        "Security boundary",
        "560025",
        "Photo",
        "0194f123-4567-7abc-8def-0123456789ab",
        &tiny_png_bytes(),
    );
    let upload = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    expect_status(
        send(&app.app, upload).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    for path in ["/health", "/api/v1/letterings"] {
        expect_status(
            send(
                &app.app,
                Request::builder().uri(path).body(Body::empty()).unwrap(),
            )
            .await,
            StatusCode::OK,
        )
        .await;
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE email=$1")
            .bind(registration_email)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM password_reset_tokens WHERE user_id=$1")
            .bind(user)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM comments WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT report_count FROM letterings WHERE id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn redis_command_failures_do_not_create_accounts() {
    let app = spawn_app().await;
    let client = redis::Client::open(std::env::var("TEST_REDIS_URL").unwrap()).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(format!("abuse:register:{}", app.client_addr.ip()))
        .arg("invalid counter")
        .arg("EX")
        .arg(10)
        .query_async(&mut connection)
        .await
        .unwrap();
    let email = unique_email("blocked-command");
    expect_status(
        send(
            &app.app,
            request(
                "/api/v1/auth/register",
                None,
                json!({"email":email,"password":"SecurePassword123!"}),
            ),
        )
        .await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE email=$1")
            .bind(email)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn trusted_proxy_chains_ignore_forged_prefixes_and_reject_invalid_addresses() {
    let app = spawn_app_with_config(|config| {
        config.trusted_proxy_cidrs = vec!["10.0.0.1/32".parse().unwrap()];
        config.limits.security.register.requests = 1;
    })
    .await;
    let bytes = *Uuid::now_v7().as_bytes();
    let source = format!("198.51.{}.{}", bytes[14], bytes[15]);
    for (prefix, status) in [
        ("203.0.113.1", StatusCode::OK),
        ("203.0.113.2", StatusCode::TOO_MANY_REQUESTS),
    ] {
        let mut req = request(
            "/api/v1/auth/register",
            None,
            json!({"email":unique_email("proxy"),"password":"SecurePassword123!"}),
        );
        req.extensions_mut()
            .insert(ConnectInfo("10.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        req.headers_mut().insert(
            "x-forwarded-for",
            format!("{prefix}, {source}").parse().unwrap(),
        );
        expect_status(send(&app.app, req).await, status).await;
    }
    let mut malformed = request(
        "/api/v1/auth/register",
        None,
        json!({"email":unique_email("bad-proxy"),"password":"SecurePassword123!"}),
    );
    malformed
        .extensions_mut()
        .insert(ConnectInfo("10.0.0.1:1234".parse::<SocketAddr>().unwrap()));
    malformed
        .headers_mut()
        .insert("x-forwarded-for", "not-an-address".parse().unwrap());
    expect_status(send(&app.app, malformed).await, StatusCode::BAD_REQUEST).await;
}

#[tokio::test]
async fn real_tcp_peers_control_limits_and_persisted_ips_even_with_spoofed_headers() {
    let app = spawn_app_with_config(|config| config.limits.security.register.requests = 1).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = app.app.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let bytes = *Uuid::now_v7().as_bytes();
    let source = IpAddr::V4(Ipv4Addr::new(127, bytes[14], bytes[15], 2));
    let client = reqwest::Client::builder()
        .no_proxy()
        .local_address(source)
        .build()
        .unwrap();
    let registered = client
        .post(format!("{origin}/api/v1/auth/register"))
        .header("host", "untrusted.example")
        .header("x-forwarded-for", "203.0.113.1")
        .json(&json!({"email":unique_email("tcp"),"password":"SecurePassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::OK);
    let account: Value = registered.json().await.unwrap();
    let response = client
        .post(format!("{origin}/api/v1/auth/register"))
        .header("x-forwarded-for", "203.0.113.2")
        .header("x-real-ip", "203.0.113.3")
        .json(&json!({"email":unique_email("tcp-spoof"),"password":"SecurePassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let (boundary, body) = multipart_upload_body(
        "TCP boundary",
        "560025",
        "Photo",
        "0194f123-4567-7abc-8def-0123456789ab",
        &tiny_png_bytes(),
    );
    let uploaded = client
        .post(format!("{origin}/api/v1/letterings/upload"))
        .bearer_auth(account["token"].as_str().unwrap())
        .header("x-forwarded-for", "203.0.113.4")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::OK);
    let upload: Value = uploaded.json().await.unwrap();
    let id = Uuid::parse_str(upload["id"].as_str().unwrap()).unwrap();
    let stored: Option<sqlx::types::ipnetwork::IpNetwork> =
        sqlx::query_scalar("SELECT uploaded_by_ip FROM letterings WHERE id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored.unwrap().ip().to_canonical(), source);
    for (spoof, expected) in [("203.0.113.5", true), ("203.0.113.6", false)] {
        let response = client
            .post(format!("{origin}/api/v1/letterings/{id}/like"))
            .header("x-forwarded-for", spoof)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let liked: Value = response.json().await.unwrap();
        assert_eq!(liked["liked"], expected);
    }
    server.abort();
}

#[tokio::test]
async fn account_limits_cannot_be_reset_by_rotating_addresses_and_competing_requests() {
    let app = spawn_app().await;
    let (token, _, id, _) = account_and_upload(&app).await;
    for (suffix, body, success) in [
        (
            "comments",
            json!({"content":"Wonderful lettering"}),
            StatusCode::OK,
        ),
        (
            "report",
            json!({"reason":"Please review"}),
            StatusCode::CREATED,
        ),
    ] {
        let mut tasks = tokio::task::JoinSet::new();
        for index in 1..=8 {
            let mut req = request(
                &format!("/api/v1/letterings/{id}/{suffix}"),
                Some(&token),
                body.clone(),
            );
            req.extensions_mut().insert(ConnectInfo(SocketAddr::new(
                Ipv4Addr::new(198, 51, 100, index).into(),
                1234,
            )));
            let router = app.app.clone();
            tasks.spawn(async move { send(&router, req).await.status() });
        }
        let mut admitted = 0;
        while let Some(status) = tasks.join_next().await {
            let status = status.unwrap();
            if status == success {
                admitted += 1;
            } else {
                assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
            }
        }
        assert_eq!(admitted, 1);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM comments WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT report_count FROM letterings WHERE id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        1
    );
}
