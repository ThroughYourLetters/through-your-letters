use super::helpers::{TestApp, expect_status, read_json, send, spawn_app};
use api::presentation::http::middleware::{admin::AdminClaims, user::UserClaims};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

struct Fixture {
    app: TestApp,
    city: Uuid,
    id: Uuid,
    token: String,
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Snapshot {
    pin_code: String,
    location: Option<String>,
    location_source: String,
    location_error: Option<String>,
    description: Option<String>,
    contributor_tag: String,
    updated_at: DateTime<Utc>,
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

async fn user(app: &TestApp) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let email = format!("metadata-{id}@example.test");
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'unused test hash')")
        .bind(id)
        .bind(&email)
        .execute(&app.db)
        .await
        .unwrap();
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &UserClaims {
            sub: id.to_string(),
            email,
            role: "USER".into(),
            exp: (Utc::now().timestamp() + 3600) as usize,
            session_version: 0,
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();
    (id, token)
}

async fn fixture(pool: &sqlx::PgPool) -> Fixture {
    let app = spawn_app(pool).await;
    let (owner, token) = user(&app).await;
    let city = Uuid::now_v7();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,$2,'IN')")
        .bind(city)
        .bind(format!("Metadata fixture {city}"))
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,location,location_source,status,description) VALUES($1,$2,$3,'Owner','image','small','medium','large','560025',ST_GeogFromText('POINT(77.61 12.95)'),'postal_code','APPROVED','Original description')")
        .bind(id).bind(city).bind(owner).execute(&app.db).await.unwrap();
    Fixture {
        app,
        city,
        id,
        token,
    }
}

impl Fixture {
    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Value,
        status: StatusCode,
    ) -> Value {
        let response = send(&self.app.app, request(method, path, token, body)).await;
        read_json(expect_status(response, status).await).await
    }
    async fn edit(&self, body: Value, status: StatusCode) -> Value {
        self.call(
            "PATCH",
            &format!("/api/v1/me/letterings/{}", self.id),
            Some(&self.token),
            body,
            status,
        )
        .await
    }
    async fn get(&self, path: &str) -> Value {
        self.call("GET", path, None, Value::Null, StatusCode::OK)
            .await
    }
    async fn snapshot(&self) -> Snapshot {
        sqlx::query_as("SELECT pin_code,ST_AsText(location) AS location,location_source,location_error,description,contributor_tag,updated_at FROM letterings WHERE id=$1")
            .bind(self.id).fetch_one(&self.app.db).await.unwrap()
    }
    async fn history(&self) -> Vec<(String, Option<String>, Option<String>)> {
        sqlx::query_as("SELECT field_name,old_value,new_value FROM lettering_metadata_history WHERE lettering_id=$1 ORDER BY created_at,id")
            .bind(self.id).fetch_all(&self.app.db).await.unwrap()
    }
}

#[sqlx::test]
async fn postal_edits_remove_warm_map_locations_and_preserve_history(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let repair = Uuid::now_v7();
    sqlx::query("INSERT INTO location_repairs(id,lettering_id,reviewed_by,before_location,after_location,evidence) VALUES($1,$2,'reviewer','{}','{}',$3)")
        .bind(repair).bind(f.id).bind(json!({"source_url":"https://example.test/postal/560025"})).execute(&f.app.db).await.unwrap();
    let gallery = format!("/api/v1/letterings?city_id={}", f.city);
    let bounds = format!("west=77&south=12&east=78&north=13&city_id={}", f.city);
    let density = format!("/api/v1/geo/density?{bounds}&zoom=15");
    let photos = format!("/api/v1/geo/photos?{bounds}");
    let summary = format!("/api/v1/geo/summary?city_id={}", f.city);
    assert_eq!(f.get(&gallery).await["letterings"][0]["pin_code"], "560025");
    assert_eq!(f.get(&density).await["cells"].as_array().unwrap().len(), 1);
    assert_eq!(f.get(&photos).await["items"].as_array().unwrap().len(), 1);
    assert_eq!(f.get(&summary).await["unmapped_count"], 0);

    let updated = f.edit(json!({"pin_code":"560001"}), StatusCode::OK).await;
    assert_eq!(updated["pin_code"], "560001");
    let snapshot = f.snapshot().await;
    assert_eq!(snapshot.location, None);
    assert_eq!(snapshot.location_source, "unresolved");
    assert_eq!(
        snapshot.location_error.as_deref(),
        Some("postal_code_changed")
    );
    let gallery = f.get(&gallery).await;
    assert_eq!(gallery["letterings"][0]["pin_code"], "560001");
    assert!(gallery["letterings"][0]["location"].is_null());
    assert_eq!(f.get(&density).await["cells"], json!([]));
    assert_eq!(f.get(&photos).await["items"], json!([]));
    assert_eq!(f.get(&summary).await["unmapped_count"], 1);
    for endpoint in ["markers", "coverage"] {
        assert_eq!(
            f.get(&format!("/api/v1/geo/{endpoint}?city_id={}", f.city))
                .await,
            json!([])
        );
    }
    assert_eq!(
        f.history().await,
        vec![(
            "pin_code".into(),
            Some("560025".into()),
            Some("560001".into())
        )]
    );
    let saved: Value = sqlx::query_scalar("SELECT evidence FROM location_repairs WHERE id=$1")
        .bind(repair)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(
        saved,
        json!({"source_url":"https://example.test/postal/560025"})
    );
}

#[sqlx::test]
async fn unchanged_postal_codes_and_other_edits_preserve_coordinates(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let original = f.snapshot().await;
    f.edit(json!({"pin_code":" 560025 "}), StatusCode::OK).await;
    assert_eq!(f.snapshot().await, original);
    assert!(f.history().await.is_empty());
    f.edit(
        json!({"description":"Updated description", "contributor_tag":"New owner"}),
        StatusCode::OK,
    )
    .await;
    let changed = f.snapshot().await;
    assert_eq!(changed.location, original.location);
    assert_eq!(changed.location_source, "postal_code");
    assert_eq!(changed.location_error, None);
    assert_eq!(changed.description.as_deref(), Some("Updated description"));
    assert_eq!(changed.contributor_tag, "New owner");
    assert_eq!(f.history().await.len(), 2);
}

#[sqlx::test]
async fn invalid_and_unauthorized_edits_leave_metadata_and_locations_unchanged(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let original = f.snapshot().await;
    for body in [
        json!({}),
        json!({"pin_code":" "}),
        json!({"pin_code":"x".repeat(21)}),
        json!({"pin_code":"560001","description":"x".repeat(1201)}),
    ] {
        f.edit(body, StatusCode::BAD_REQUEST).await;
        assert_eq!(f.snapshot().await, original);
    }
    let (_, other) = user(&f.app).await;
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(other.as_str()), StatusCode::FORBIDDEN),
    ] {
        f.call(
            "PATCH",
            &format!("/api/v1/me/letterings/{}", f.id),
            token,
            json!({"pin_code":"560001"}),
            status,
        )
        .await;
    }
    assert_eq!(f.snapshot().await, original);
    assert!(f.history().await.is_empty());
}

#[sqlx::test]
async fn history_failures_roll_back_postal_and_location_changes(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let original = f.snapshot().await;
    let constraint = format!("fail_metadata_{}", f.id.simple());
    sqlx::query(&format!("ALTER TABLE lettering_metadata_history ADD CONSTRAINT {constraint} CHECK (lettering_id <> '{}') NOT VALID", f.id))
        .execute(&f.app.db).await.unwrap();
    let response = send(
        &f.app.app,
        request(
            "PATCH",
            &format!("/api/v1/me/letterings/{}", f.id),
            Some(&f.token),
            json!({"pin_code":"560001","description":"Changed"}),
        ),
    )
    .await;
    sqlx::query(&format!(
        "ALTER TABLE lettering_metadata_history DROP CONSTRAINT {constraint}"
    ))
    .execute(&f.app.db)
    .await
    .unwrap();
    expect_status(response, StatusCode::INTERNAL_SERVER_ERROR).await;
    assert_eq!(f.snapshot().await, original);
    assert!(f.history().await.is_empty());
}

#[sqlx::test]
async fn edits_read_committed_metadata_after_waiting_for_a_competing_writer(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let mut competing = f.app.db.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *competing)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET pin_code='560001',location=ST_GeogFromText('POINT(77.59 12.97)'),contributor_tag='Other editor',updated_at=clock_timestamp() WHERE id=$1")
        .bind(f.id).execute(&mut *competing).await.unwrap();
    let app = f.app.app.clone();
    let edit = request(
        "PATCH",
        &format!("/api/v1/me/letterings/{}", f.id),
        Some(&f.token),
        json!({"description":"Concurrent description"}),
    );
    let task = tokio::spawn(async move { send(&app, edit).await });
    // Wait for the actual lock conflict.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("metadata edit never waited for the competing writer");
    let written_at: DateTime<Utc> = sqlx::query_scalar(
        "UPDATE letterings SET updated_at=clock_timestamp() WHERE id=$1 RETURNING updated_at",
    )
    .bind(f.id)
    .fetch_one(&mut *competing)
    .await
    .unwrap();
    competing.commit().await.unwrap();
    expect_status(task.await.unwrap(), StatusCode::OK).await;
    let current = f.snapshot().await;
    assert!(current.updated_at > written_at);
    assert_eq!(current.pin_code, "560001");
    assert_eq!(current.location.as_deref(), Some("POINT(77.59 12.97)"));
    assert_eq!(current.location_source, "postal_code");
    assert_eq!(current.contributor_tag, "Other editor");
    assert_eq!(
        current.description.as_deref(),
        Some("Concurrent description")
    );
    assert_eq!(
        f.history().await,
        vec![(
            "description".into(),
            Some("Original description".into()),
            Some("Concurrent description".into())
        )]
    );
}

#[sqlx::test]
async fn stale_repairs_cannot_restore_a_previous_postal_location(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    let admin = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &AdminClaims {
            sub: "reviewer@example.test".into(),
            exp: (Utc::now().timestamp() + 3600) as usize,
            aud: "admin".into(),
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-admin-jwt-secret"),
    )
    .unwrap();
    let root = format!("/api/v1/admin/location-repairs/{}", f.id);
    let mut evidence = json!({"lat":12.96,"lng":77.62,"city_name":format!("Metadata fixture {}",f.city),"country_code":"IN","pin_code":"560025","source_name":"Postal fixture","source_url":"https://example.test/postal","reason":"Verified against postal source"});
    let preview = f
        .call(
            "POST",
            &format!("{root}/preview"),
            Some(&admin),
            evidence.clone(),
            StatusCode::OK,
        )
        .await;
    f.edit(json!({"pin_code":"560001"}), StatusCode::OK).await;
    let approval = |preview: &Value| json!({"repair_id":preview["repair_id"],"expected_updated_at":preview["current"]["updated_at"],"evidence":preview["evidence"],"reviewed":true});
    f.call(
        "POST",
        &format!("{root}/apply"),
        Some(&admin),
        approval(&preview),
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(f.snapshot().await.location, None);
    evidence["pin_code"] = json!("560001");
    let preview = f
        .call(
            "POST",
            &format!("{root}/preview"),
            Some(&admin),
            evidence,
            StatusCode::OK,
        )
        .await;
    f.call(
        "POST",
        &format!("{root}/apply"),
        Some(&admin),
        approval(&preview),
        StatusCode::OK,
    )
    .await;
    let current = f.snapshot().await;
    assert_eq!(current.pin_code, "560001");
    assert_eq!(current.location.as_deref(), Some("POINT(77.62 12.96)"));
    assert_eq!(current.location_source, "postal_code");
    assert_eq!(current.location_error, None);
}

#[sqlx::test]
async fn unresolved_locations_do_not_reuse_coordinates_after_postal_edits(pool: sqlx::PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("UPDATE letterings SET location=NULL,location_source='unresolved',location_error='provider_unavailable' WHERE id=$1")
        .bind(f.id).execute(&f.app.db).await.unwrap();
    let unresolved = f.snapshot().await;
    f.edit(json!({"pin_code":"560025"}), StatusCode::OK).await;
    assert_eq!(f.snapshot().await, unresolved);
    f.edit(json!({"pin_code":"560001"}), StatusCode::OK).await;
    let current = f.snapshot().await;
    assert_eq!(current.location, None);
    assert_eq!(current.location_source, "unresolved");
    assert_eq!(
        current.location_error.as_deref(),
        Some("postal_code_changed")
    );
}
