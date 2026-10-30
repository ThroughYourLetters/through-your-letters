use super::helpers::{TestApp, expect_status, read_json, send, spawn_app};
use api::presentation::http::middleware::admin::AdminClaims;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use uuid::Uuid;

fn admin_token(audience: &str) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AdminClaims {
            sub: "location-reviewer@example.test".into(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            aud: audience.into(),
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap()
}
fn request(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request.body(Body::from(body.to_string())).unwrap()
}
async fn fixture(app: &TestApp) -> (Uuid, Uuid) {
    let city = Uuid::now_v7();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,$2,'IN')")
        .bind(city)
        .bind(format!("Repair fixture {city}"))
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status) VALUES($1,$2,'Reviewer','image','small','medium','large',ST_GeogFromText('POINT(77 12)'),'560025','APPROVED')").bind(id).bind(city).execute(&app.db).await.unwrap();
    (city, id)
}
fn evidence(city: Uuid) -> Value {
    json!({"lat":12.95,"lng":77.61,"city_name":format!("Repair fixture {city}"),"country_code":"in","pin_code":"560 025","source_name":"Postal authority fixture","source_url":"https://example.test/postal/560025","reason":"Checked the postal centroid against the source"})
}
async fn call(
    app: &TestApp,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
    status: StatusCode,
) -> Value {
    let response = send(
        &app.app,
        request(method, path, (!token.is_empty()).then_some(token), body),
    )
    .await;
    read_json(expect_status(response, status).await).await
}

#[sqlx::test]
async fn repairs_require_admin_preview_review_and_keep_history(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let (city, id) = fixture(&app).await;
    let token = admin_token("admin");
    let root = format!("/api/v1/admin/location-repairs/{id}");
    for path in [
        "/api/v1/admin/location-repairs".to_string(),
        format!("{root}/history"),
    ] {
        for token in [None, Some(admin_token("user"))] {
            let response = send(
                &app.app,
                request("GET", &path, token.as_deref(), Value::Null),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }
    for action in ["preview", "apply"] {
        let response = send(
            &app.app,
            request("POST", &format!("{root}/{action}"), None, evidence(city)),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    call(
        &app,
        "GET",
        &format!("/api/v1/letterings?city_id={city}"),
        "",
        Value::Null,
        StatusCode::OK,
    )
    .await;
    let before: (String, String, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT ST_AsText(location),location_source,updated_at FROM letterings WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let preview = call(
        &app,
        "POST",
        &format!("{root}/preview"),
        &token,
        evidence(city),
        StatusCode::OK,
    )
    .await;
    let after_preview: (String, String, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT ST_AsText(location),location_source,updated_at FROM letterings WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(before, after_preview);
    let mut approval = json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":false});
    call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval.clone(),
        StatusCode::BAD_REQUEST,
    )
    .await;
    approval["reviewed"] = json!(true);
    let first = call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval.clone(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        first["before_location"]["location_source"],
        "legacy_unverified"
    );
    assert_eq!(first["after_location"]["location_source"], "postal_code");
    assert_eq!(first["after_location"]["lng"], 77.61);
    let gallery = call(
        &app,
        "GET",
        &format!("/api/v1/letterings?city_id={city}"),
        "",
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        gallery["letterings"][0]["location"]["coordinates"],
        json!([77.61, 12.95])
    );

    let repeated = call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval.clone(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(first, repeated);
    let history = call(
        &app,
        "GET",
        &format!("{root}/history"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert!(history["next_cursor"].is_null());
    approval["evidence"]["lat"] = json!(52.0);
    call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval,
        StatusCode::CONFLICT,
    )
    .await;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_audit_logs WHERE lettering_id=$1 AND action='location_repair'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let report = call(
        &app,
        "GET",
        &format!("/api/v1/admin/location-repairs?city_id={city}"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(report["items"], json!([]));
    sqlx::query("DELETE FROM letterings WHERE id=$1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    let history = call(
        &app,
        "GET",
        &format!("{root}/history"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn repairs_reject_invalid_sources_stale_previews_and_competing_changes(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let (city, id) = fixture(&app).await;
    let token = admin_token("admin");
    let root = format!("/api/v1/admin/location-repairs/{id}");
    for (key, bad) in [
        ("lat", json!(91)),
        ("lng", json!(181)),
        ("city_name", json!("Wrong city")),
        ("country_code", json!("GB")),
        ("pin_code", json!("560001")),
        ("source_name", json!("")),
        ("reason", json!("")),
        ("source_url", json!("javascript:alert(1)")),
        ("source_url", json!("https://user:password@example.test/")),
        ("reason", json!("x".repeat(4097))),
    ] {
        let mut input = evidence(city);
        input[key] = bad;
        call(
            &app,
            "POST",
            &format!("{root}/preview"),
            &token,
            input,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    let preview = call(
        &app,
        "POST",
        &format!("{root}/preview"),
        &token,
        evidence(city),
        StatusCode::OK,
    )
    .await;
    let approval = json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":true});
    sqlx::query("UPDATE letterings SET updated_at=updated_at+INTERVAL '1 second' WHERE id=$1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval,
        StatusCode::CONFLICT,
    )
    .await;
    let preview = call(
        &app,
        "POST",
        &format!("{root}/preview"),
        &token,
        evidence(city),
        StatusCode::OK,
    )
    .await;
    let approval = json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":true});
    let mut second = approval.clone();
    second["repair_id"] = json!(Uuid::now_v7());
    let (a, b) = tokio::join!(
        send(
            &app.app,
            request("POST", &format!("{root}/apply"), Some(&token), approval)
        ),
        send(
            &app.app,
            request("POST", &format!("{root}/apply"), Some(&token), second)
        )
    );
    let mut statuses = [a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM location_repairs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn audit_failures_roll_back_coordinates_and_repair_history(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let (city, id) = fixture(&app).await;
    let token = admin_token("admin");
    let root = format!("/api/v1/admin/location-repairs/{id}");
    let preview = call(
        &app,
        "POST",
        &format!("{root}/preview"),
        &token,
        evidence(city),
        StatusCode::OK,
    )
    .await;
    let approval = json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":true});
    let name = format!("test_repair_{}", id.simple());
    sqlx::raw_sql(&format!("CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture audit failure'; END $$; CREATE TRIGGER {name} BEFORE INSERT ON admin_audit_logs FOR EACH ROW WHEN (NEW.lettering_id='{id}'::uuid) EXECUTE FUNCTION {name}();")).execute(&app.db).await.unwrap();
    let response = send(
        &app.app,
        request(
            "POST",
            &format!("{root}/apply"),
            Some(&token),
            approval.clone(),
        ),
    )
    .await;
    sqlx::raw_sql(&format!(
        "DROP TRIGGER {name} ON admin_audit_logs; DROP FUNCTION {name}();"
    ))
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let source: String = sqlx::query_scalar("SELECT location_source FROM letterings WHERE id=$1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(source, "legacy_unverified");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM location_repairs WHERE lettering_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 0);
    call(
        &app,
        "POST",
        &format!("{root}/apply"),
        &token,
        approval,
        StatusCode::OK,
    )
    .await;
}

#[sqlx::test]
async fn report_and_history_use_stable_cursors(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let (city, id) = fixture(&app).await;
    let token = admin_token("admin");
    let second = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status) SELECT $1,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status FROM letterings WHERE id=$2").bind(second).bind(id).execute(&app.db).await.unwrap();
    let root = format!("/api/v1/admin/location-repairs/{id}");
    let report = call(
        &app,
        "GET",
        &format!("/api/v1/admin/location-repairs?city_id={city}&limit=1"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(report["items"][0]["id"], id.to_string());
    assert_eq!(report["next_cursor"], id.to_string());
    let next = call(
        &app,
        "GET",
        &format!("/api/v1/admin/location-repairs?city_id={city}&limit=1&after={id}"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(next["items"][0]["id"], second.to_string());
    assert!(next["next_cursor"].is_null());
    let mut ids = vec![];
    for lat in [12.95, 12.951] {
        let mut input = evidence(city);
        input["lat"] = json!(lat);
        let preview = call(
            &app,
            "POST",
            &format!("{root}/preview"),
            &token,
            input,
            StatusCode::OK,
        )
        .await;
        let approval = json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":true});
        let saved = call(
            &app,
            "POST",
            &format!("{root}/apply"),
            &token,
            approval,
            StatusCode::OK,
        )
        .await;
        ids.push(saved["id"].as_str().unwrap().to_string());
    }
    let report = call(
        &app,
        "GET",
        &format!("/api/v1/admin/location-repairs?city_id={city}&include_resolved=true"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(report["items"].as_array().unwrap().len(), 2);
    let history = call(
        &app,
        "GET",
        &format!("{root}/history?limit=1"),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(history["items"][0]["id"], ids[1]);
    assert_eq!(history["next_cursor"], ids[1]);
    let older = call(
        &app,
        "GET",
        &format!("{root}/history?limit=1&after={}", ids[1]),
        &token,
        Value::Null,
        StatusCode::OK,
    )
    .await;
    assert_eq!(older["items"][0]["id"], ids[0]);
    assert!(older["next_cursor"].is_null());
}
