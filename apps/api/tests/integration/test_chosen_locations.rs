use super::{
    helpers::{
        expect_status, multipart_upload_body, read_json, send, spawn_app_with_geocoding_reply,
        tiny_png_bytes,
    },
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn point() -> Value {
    json!({"latitude":51.5074,"longitude":-0.1278,"place_name":"Test square","accuracy_m":100.0})
}
fn edit(f: &Fixture, id: Uuid, body: Value) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/me/letterings/{id}"))
        .header("authorization", format!("Bearer {}", f.token))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
async fn patch(f: &Fixture, body: Value, expected: StatusCode) -> Value {
    read_json(expect_status(send(&f.app.app, edit(f, f.ids[0], body)).await, expected).await).await
}
fn upload(city: Uuid, point: Value, key: Uuid, image: &[u8]) -> Request<Body> {
    let (boundary, mut body) = multipart_upload_body(
        "Map contributor",
        "SW1A 1AA",
        "A painted shop sign",
        &city.to_string(),
        image,
    );
    let closing = format!("--{boundary}--\r\n");
    assert!(body.ends_with(closing.as_bytes()));
    body.truncate(body.len() - closing.len());
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"location\"\r\n\r\n{point}\r\n{closing}").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("idempotency-key", key.to_string())
        .body(Body::from(body))
        .unwrap()
}

#[sqlx::test]
async fn chosen_upload_pins_survive_reads_and_idempotent_retries(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    f.app.stop_geocoder();
    let city: Uuid = sqlx::query_scalar("SELECT city_id FROM letterings WHERE id=$1")
        .bind(f.ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    let key = Uuid::now_v7();
    let image = tiny_png_bytes();
    let saved: Value = read_json(
        expect_status(
            send(&f.app.app, upload(city, point(), key, &image)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(saved["location_source"], "contributor_pin");
    assert!(saved["location_warning"].is_null());
    let id = saved["id"].as_str().unwrap();
    let detail = f.get(&format!("/api/v1/letterings/{id}"), false).await;
    assert_eq!(detail["location"]["coordinates"], json!([-0.1278, 51.5074]));
    assert_eq!(detail["location_name"], "Test square");
    assert_eq!(detail["location_accuracy_m"], 100.0);
    let retry: Value = read_json(
        expect_status(
            send(&f.app.app, upload(city, point(), key, &image)).await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(retry["id"], saved["id"]);
    let mut changed = point();
    changed["longitude"] = (-0.12).into();
    expect_status(
        send(&f.app.app, upload(city, changed, key, &image)).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        f.app
            .storage_uploads
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
}

#[sqlx::test]
async fn owners_can_correct_clear_and_replace_pins_without_stale_map_data(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let response = patch(
        &f,
        json!({"location":{"action":"set","point":point()}}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(response["latitude"], 51.5074);
    assert_eq!(response["longitude"], -0.1278);
    assert_eq!(response["location_source"], "contributor_pin");
    let density = "/api/v1/geo/density?west=-1&south=50&east=1&north=52&zoom=12";
    let count = |body: &Value| {
        body["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell["count"].as_i64().unwrap())
            .sum::<i64>()
    };
    assert_eq!(count(&f.get(density, false).await), 1);
    assert_eq!(
        f.get("/api/v1/geo/photos?west=-1&south=50&east=1&north=52", false)
            .await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    sqlx::query("ALTER TABLE lettering_metadata_history ADD CONSTRAINT refuse_pin_history CHECK(field_name<>'location') NOT VALID").execute(&pool).await.unwrap();
    patch(
        &f,
        json!({"location":{"action":"clear"}}),
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    let retained = f
        .get(&format!("/api/v1/letterings/{}", f.ids[0]), false)
        .await;
    assert_eq!(retained["location_name"], "Test square");
    assert_eq!(retained["location_accuracy_m"], 100.0);
    assert_eq!(retained["updated_at"], response["updated_at"]);
    sqlx::query("ALTER TABLE lettering_metadata_history DROP CONSTRAINT refuse_pin_history")
        .execute(&pool)
        .await
        .unwrap();
    let cleared = patch(&f, json!({"location":{"action":"clear"}}), StatusCode::OK).await;
    assert_eq!(cleared["location_source"], "unresolved");
    assert!(
        cleared["latitude"].is_null()
            && cleared["location_name"].is_null()
            && cleared["location_accuracy_m"].is_null()
    );
    assert_eq!(count(&f.get(density, false).await), 0);
    let mut unknown_accuracy = point();
    unknown_accuracy["accuracy_m"] = Value::Null;
    let replaced = patch(
        &f,
        json!({"pin_code":"10001","location":{"action":"set","point":unknown_accuracy}}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(replaced["location_source"], "contributor_pin");
    assert!(replaced["location_accuracy_m"].is_null());
    let postal = patch(&f, json!({"pin_code":"10002"}), StatusCode::OK).await;
    assert_eq!(postal["location_source"], "unresolved");
    assert!(postal["latitude"].is_null() && postal["location_name"].is_null());
    let history: i64 = sqlx::query_scalar("SELECT count(*) FROM lettering_metadata_history WHERE lettering_id=$1 AND field_name='location'").bind(f.ids[0]).fetch_one(&pool).await.unwrap();
    assert_eq!(history, 3);
}

#[sqlx::test]
async fn invalid_pins_and_other_accounts_cannot_change_upload_locations(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    for (field, value) in [
        ("latitude", json!(91)),
        ("longitude", json!(-181)),
        ("place_name", json!(" ")),
        ("place_name", json!("x".repeat(201))),
        ("place_name", json!("a\nb")),
        ("accuracy_m", json!(0)),
        ("accuracy_m", json!(1000001)),
    ] {
        let mut invalid = point();
        invalid[field] = value;
        patch(
            &f,
            json!({"location":{"action":"set","point":invalid}}),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    let other = Uuid::now_v7();
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &api::presentation::http::middleware::user::UserClaims {
            sub: other.to_string(),
            email: "other@example.test".into(),
            role: "USER".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            session_version: 0,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'test')")
        .bind(other)
        .bind(format!("{other}@example.test"))
        .execute(&pool)
        .await
        .unwrap();
    let mut request = edit(
        &f,
        f.ids[0],
        json!({"location":{"action":"set","point":point()}}),
    );
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    expect_status(send(&f.app.app, request).await, StatusCode::FORBIDDEN).await;
    let detail = f
        .get(&format!("/api/v1/letterings/{}", f.ids[0]), false)
        .await;
    assert!(detail["location"].is_null());
}

#[sqlx::test]
async fn explicit_place_searches_share_cache_and_enforce_request_budgets(pool: PgPool) {
    let app = spawn_app_with_geocoding_reply(&pool, vec![json!({"name":"Test square","display_name":"Test square, London","lat":"51.5074","lon":"-0.1278"})], |_| {}).await;
    let request = |path: &str| Request::builder().uri(path).body(Body::empty()).unwrap();
    let first: Value = read_json(
        expect_status(
            send(
                &app.app,
                request("/api/v1/places?q=Test%20square&country_code=GB"),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(
        first,
        json!([{"name":"Test square, London","latitude":51.5074,"longitude":-0.1278}])
    );
    app.stop_geocoder();
    let cached: Value = read_json(
        expect_status(
            send(
                &app.app,
                request("/api/v1/places?q=test%20square&country_code=gb"),
            )
            .await,
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(first, cached);
    for path in [
        "/api/v1/places?q=x",
        "/api/v1/places?q=valid&limit=11",
        "/api/v1/places?q=valid&country_code=INVALID",
    ] {
        expect_status(send(&app.app, request(path)).await, StatusCode::BAD_REQUEST).await;
    }
    expect_status(
        send(&app.app, request("/api/v1/places?q=Different")).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
}

#[sqlx::test]
async fn place_search_limits_fail_closed_and_leave_other_public_reads_available(pool: PgPool) {
    let request = |path: &str| Request::builder().uri(path).body(Body::empty()).unwrap();
    let app = spawn_app_with_geocoding_reply(&pool, vec![], |config| {
        config.limits.security.place_search.requests = 1
    })
    .await;
    expect_status(
        send(&app.app, request("/api/v1/places?q=London")).await,
        StatusCode::OK,
    )
    .await;
    let response = send(&app.app, request("/api/v1/places?q=Oxford")).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    drop(app);
    let offline = super::helpers::spawn_app_with_config(&pool, |config| {
        config.redis_url = "redis://127.0.0.1:9".into();
        config.limits.security.redis_connect_timeout_ms = 100;
        config.limits.security.redis_response_timeout_ms = 100;
    })
    .await;
    expect_status(
        send(&offline.app, request("/api/v1/places?q=London")).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    expect_status(
        send(&offline.app, request("/api/v1/letterings")).await,
        StatusCode::OK,
    )
    .await;
}
