use super::helpers::{TestApp, expect_status, read_json, send, spawn_app, unique_email};
use api::presentation::http::middleware::{admin::AdminClaims, user::UserClaims};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures_util::StreamExt;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

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

async fn user(app: &TestApp) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let email = unique_email("visibility");
    sqlx::query("INSERT INTO users(id,email,password_hash,role) VALUES($1,$2,'unused','USER')")
        .bind(id)
        .bind(&email)
        .execute(&app.db)
        .await
        .unwrap();
    let claims = UserClaims {
        sub: id.to_string(),
        email,
        role: "USER".into(),
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        session_version: 0,
    };
    (
        id,
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"test-jwt-secret"),
        )
        .unwrap(),
    )
}

async fn photo(app: &TestApp, city: Uuid, owner: Uuid, tag: &str, status: &str, pin: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,location_source,pin_code,status,detected_text,ml_style,ml_script) VALUES($1,$2,$3,$4,$5,$5,$5,$5,ST_GeogFromText('POINT(71 19)'),'postal_code',$6,$7,$4,$4,$4)")
        .bind(id).bind(city).bind(owner).bind(tag).bind(format!("https://visibility.example.test/{id}.webp"))
        .bind(pin).bind(status).execute(&app.db).await.unwrap();
    id
}

struct Fixture {
    app: TestApp,
    owner: String,
    other: String,
    other_id: Uuid,
    admin: String,
    city: Uuid,
    hidden_city: Uuid,
    root: Uuid,
    candidate: Uuid,
    hidden: Vec<Uuid>,
    collection: Uuid,
    tag: String,
}

impl Fixture {
    async fn new(country: &str) -> Self {
        let app = spawn_app().await;
        let (owner_id, owner) = user(&app).await;
        let (other_id, other) = user(&app).await;
        let admin = encode(
            &Header::default(),
            &AdminClaims {
                sub: "visibility-admin".into(),
                exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
                aud: "admin".into(),
            },
            &EncodingKey::from_secret(b"test-admin-jwt-secret"),
        )
        .unwrap();
        let city = Uuid::now_v7();
        let hidden_city = Uuid::now_v7();
        for (id, code) in [(city, country), (hidden_city, "QH")] {
            sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,$2,$3)")
                .bind(id)
                .bind(format!("Visibility {id}"))
                .bind(code)
                .execute(&app.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO region_policies(country_code,discoverability_enabled) VALUES('QH',false) ON CONFLICT(country_code) DO UPDATE SET discoverability_enabled=false")
            .execute(&app.db).await.unwrap();
        let tag = format!("v{}", &Uuid::now_v7().simple().to_string()[..20]);
        let root = photo(&app, city, owner_id, &tag, "APPROVED", "970001").await;
        let candidate = photo(&app, city, owner_id, &tag, "APPROVED", "970002").await;
        let mut hidden = Vec::new();
        for (index, status) in ["PENDING", "REJECTED", "REPORTED"].into_iter().enumerate() {
            hidden.push(
                photo(
                    &app,
                    city,
                    owner_id,
                    &tag,
                    status,
                    &format!("97000{}", index + 3),
                )
                .await,
            );
        }
        hidden.push(photo(&app, hidden_city, owner_id, &tag, "APPROVED", "970006").await);
        let collection = Uuid::now_v7();
        sqlx::query("INSERT INTO collections(id,name,creator_tag,is_public,owner_user_id) VALUES($1,$2,$2,true,$3)")
            .bind(collection).bind(&tag).bind(owner_id).execute(&app.db).await.unwrap();
        for id in [root, candidate].into_iter().chain(hidden.iter().copied()) {
            sqlx::query("INSERT INTO collection_items(collection_id,lettering_id) VALUES($1,$2)")
                .bind(collection)
                .bind(id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        Self {
            app,
            owner,
            other,
            other_id,
            admin,
            city,
            hidden_city,
            root,
            candidate,
            hidden,
            collection,
            tag,
        }
    }

    async fn get(&self, path: &str, token: Option<&str>) -> Value {
        let response = send(&self.app.app, request("GET", path, token, Value::Null)).await;
        assert_eq!(response.status(), StatusCode::OK, "GET {path}");
        read_json(response).await
    }

    fn assert_no_hidden(&self, body: &Value) {
        for id in &self.hidden {
            assert!(
                !body.to_string().contains(&id.to_string()),
                "hidden upload escaped through a public response"
            );
        }
    }
}

#[tokio::test]
async fn discovery_and_counts_share_moderation_and_region_policy() {
    let f = Fixture::new("QA").await;
    let paths = [
        format!("/api/v1/letterings?city_id={}", f.city),
        format!("/api/v1/letterings/search?q={}", f.tag),
        format!("/api/v1/contributors/{}", f.tag),
        format!("/api/v1/letterings/{}/similar", f.root),
        format!("/api/v1/collections/{}", f.collection),
        format!("/api/v1/geo/markers?city_id={}", f.city),
        "/api/v1/geo/nearby?lat=19&lng=71&radius_m=1000".into(),
        format!(
            "/api/v1/geo/photos?west=70&south=18&east=72&north=20&city_id={}",
            f.city
        ),
    ];
    for token in [None, Some(f.owner.as_str()), Some(f.other.as_str())] {
        for path in &paths {
            f.assert_no_hidden(&f.get(path, token).await);
        }
        let contributor = f
            .get(&format!("/api/v1/contributors/{}", f.tag), token)
            .await;
        assert_eq!(contributor["total_count"], 2);
        let collection = f
            .get(&format!("/api/v1/collections/{}", f.collection), token)
            .await;
        assert_eq!(collection["total_items"], 2);
        let collections = f.get("/api/v1/collections?limit=100", token).await;
        let entry = collections["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == f.collection.to_string())
            .unwrap();
        assert_eq!(entry["item_count"], 2);
        let leaderboard = f.get("/api/v1/community/leaderboard", token).await;
        let entry = leaderboard
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["tag"] == f.tag)
            .unwrap();
        assert_eq!(entry["count"], 2);
        for city in [f.city, f.hidden_city] {
            let expected = if city == f.city { 2 } else { 0 };
            let details = f.get(&format!("/api/v1/cities/{city}"), token).await;
            assert_eq!(details["lettering_count"], expected);
            let stats = f.get(&format!("/api/v1/cities/{city}/stats"), token).await;
            assert_eq!(stats.as_array().unwrap().len(), expected as usize);
            let coverage = f
                .get(&format!("/api/v1/geo/coverage?city_id={city}"), token)
                .await;
            assert_eq!(coverage.as_array().unwrap().len(), expected as usize);
            let density = f.get(&format!("/api/v1/geo/density?west=70&south=18&east=72&north=20&zoom=12&city_id={city}"), token).await;
            assert_eq!(
                density["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["count"].as_i64().unwrap())
                    .sum::<i64>(),
                expected
            );
        }
    }
    let neighborhoods = f.get("/api/v1/analytics/neighborhoods", None).await;
    for pin in ["970003", "970004", "970005", "970006"] {
        assert!(!neighborhoods.to_string().contains(pin));
    }
}

#[tokio::test]
async fn direct_and_encoded_reads_keep_owner_previews_and_admin_review_separate() {
    let f = Fixture::new("QB").await;
    for id in &f.hidden {
        for suffix in ["", "/download", "/comments", "/similar", "/revisits"] {
            for spelling in [id.to_string(), id.to_string().replace('-', "%2D")] {
                for (token, status) in [
                    (None, StatusCode::NOT_FOUND),
                    (Some(f.other.as_str()), StatusCode::NOT_FOUND),
                    (
                        Some(f.owner.as_str()),
                        if suffix == "/download" {
                            StatusCode::TEMPORARY_REDIRECT
                        } else {
                            StatusCode::OK
                        },
                    ),
                    (Some(f.admin.as_str()), StatusCode::UNAUTHORIZED),
                ] {
                    expect_status(
                        send(
                            &f.app.app,
                            request(
                                "GET",
                                &format!("/api/v1/letterings/{spelling}{suffix}"),
                                token,
                                Value::Null,
                            ),
                        )
                        .await,
                        status,
                    )
                    .await;
                }
            }
        }
    }
    let review = f
        .get(
            "/api/v1/admin/moderation?status=REJECTED&limit=200",
            Some(&f.admin),
        )
        .await;
    assert!(review.to_string().contains(&f.hidden[1].to_string()));
    expect_status(
        send(
            &f.app.app,
            request(
                "GET",
                "/api/v1/admin/moderation",
                Some(&f.owner),
                Value::Null,
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[tokio::test]
async fn linked_uploads_cannot_bypass_visibility_and_moderation_invalidates_cached_discovery() {
    let f = Fixture::new("QC").await;
    for target in [f.candidate].into_iter().chain(f.hidden.iter().copied()) {
        sqlx::query("INSERT INTO location_revisits(id,original_lettering_id,revisit_lettering_id,notes) VALUES($1,$2,$3,$4)")
            .bind(Uuid::now_v7()).bind(f.root).bind(target).bind(format!("Revisit {target}"))
            .execute(&f.app.db).await.unwrap();
    }
    for token in [None, Some(f.other.as_str())] {
        let body = f
            .get(&format!("/api/v1/letterings/{}/revisits", f.root), token)
            .await;
        f.assert_no_hidden(&body);
        assert_eq!(body["revisits"].as_array().unwrap().len(), 1);
    }
    let previews = f
        .get(
            &format!("/api/v1/letterings/{}/revisits", f.root),
            Some(&f.owner),
        )
        .await;
    assert_eq!(previews["revisits"].as_array().unwrap().len(), 5);
    let unrelated_hidden = photo(&f.app, f.city, f.other_id, &f.tag, "REJECTED", "970007").await;
    for id in [unrelated_hidden, Uuid::now_v7()] {
        expect_status(
            send(
                &f.app.app,
                request(
                    "POST",
                    &format!("/api/v1/letterings/{}/revisits", f.root),
                    Some(&f.owner),
                    json!({"revisit_lettering_id":id}),
                ),
            )
            .await,
            StatusCode::NOT_FOUND,
        )
        .await;
        expect_status(
            send(
                &f.app.app,
                request(
                    "POST",
                    &format!("/api/v1/collections/{}/items/{id}", f.collection),
                    Some(&f.owner),
                    Value::Null,
                ),
            )
            .await,
            StatusCode::NOT_FOUND,
        )
        .await;
    }
    let gallery = format!("/api/v1/letterings?city_id={}", f.city);
    assert_eq!(f.get(&gallery, None).await["total"], 2);
    expect_status(
        send(
            &f.app.app,
            request(
                "POST",
                &format!("/api/v1/admin/letterings/{}/reject", f.root),
                Some(&f.admin),
                json!({"reason":"Visibility regression"}),
            ),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let after = f.get(&gallery, None).await;
    assert_eq!(after["total"], 1);
    assert!(!after.to_string().contains(&f.root.to_string()));
}

#[tokio::test]
async fn anonymous_websocket_feed_never_discloses_hidden_upload_ids() {
    let f = Fixture::new("QD").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/ws/feed", listener.local_addr().unwrap());
    let router = f.app.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (mut socket, _) = tokio_tungstenite::connect_async(endpoint).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.app.ws_broadcaster.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    for id in f.hidden.iter().copied().chain([Uuid::now_v7()]) {
        f.app
            .ws_broadcaster
            .send(json!({"type":"PROCESSED", "id":id}).to_string())
            .unwrap();
    }
    f.app
        .ws_broadcaster
        .send(json!({"type":"PROCESSED", "id":f.root, "private_extra":f.hidden[0]}).to_string())
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap(),
        json!({"type":"PROCESSED", "id":f.root})
    );

    sqlx::query("UPDATE letterings SET status='REJECTED' WHERE id=$1")
        .bind(f.root)
        .execute(&f.app.db)
        .await
        .unwrap();
    for id in [f.root, f.candidate] {
        f.app
            .ws_broadcaster
            .send(json!({"type":"PROCESSED", "id":id}).to_string())
            .unwrap();
    }
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap(),
        json!({"type":"PROCESSED", "id":f.candidate})
    );
    server.abort();
}
