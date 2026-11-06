use super::helpers::{TestApp, read_json, send, spawn_app, spawn_app_with_config};
use api::presentation::http::middleware::{admin::AdminClaims, user::UserClaims};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeZone, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub(super) struct Fixture {
    pub(super) app: TestApp,
    pub(super) token: String,
    pub(super) admin: String,
    pub(super) ids: Vec<Uuid>,
    pub(super) collection: Uuid,
}
impl Fixture {
    pub(super) async fn new(pool: &PgPool) -> Self {
        let app = spawn_app(pool).await;
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
        let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let ids: Vec<_> = (1000..1008).map(Uuid::from_u128).collect();
        for &id in &ids {
            sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status,created_at) VALUES($1,$2,$3,'PageTester','image','small','medium','large','560001','APPROVED',$4)")
                .bind(id).bind(city).bind(user).bind(time).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO notifications(id,user_id,type,title,created_at) VALUES($1,$2,'test','Page notice',$3)")
                .bind(id).bind(user).bind(time).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,created_at) VALUES($1,'test-admin','PAGE_TEST',$2)")
                .bind(id).bind(time).execute(pool).await.unwrap();
        }
        let collection = Uuid::from_u128(2000);
        sqlx::query("INSERT INTO collections(id,name,creator_tag,is_public,owner_user_id,created_at) VALUES($1,'Page collection','PageTester',true,$2,$3)")
            .bind(collection).bind(user).bind(time).execute(pool).await.unwrap();
        for &id in &ids {
            sqlx::query("INSERT INTO collection_items(collection_id,lettering_id) VALUES($1,$2)")
                .bind(collection)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
        }
        let exp = (Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;
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
                sub: "test-admin".into(),
                exp,
                aud: "admin".into(),
            },
            &EncodingKey::from_secret(b"test-admin-jwt-secret"),
        )
        .unwrap();
        Self {
            app,
            token,
            admin,
            ids,
            collection,
        }
    }
    pub(super) async fn response(&self, path: &str, admin: bool) -> axum::response::Response {
        send(
            &self.app.app,
            Request::builder()
                .uri(path)
                .header(
                    "authorization",
                    format!("Bearer {}", if admin { &self.admin } else { &self.token }),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }
    pub(super) async fn get(&self, path: &str, admin: bool) -> Value {
        let response = self.response(path, admin).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        read_json(response).await
    }
}
fn ids(value: &Value, field: Option<&str>) -> Vec<Uuid> {
    let rows = field.map_or(value, |field| &value[field]);
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

#[sqlx::test]
async fn lists_reject_invalid_limits_and_offsets_and_accept_their_existing_maxima(pool: PgPool) {
    let fixture = Fixture::new(&pool).await;
    let collection = format!("/api/v1/collections/{}", fixture.collection);
    let history = format!("/api/v1/admin/location-repairs/{}/history", fixture.ids[0]);
    let endpoints = [
        ("/api/v1/letterings", 100, false, true),
        ("/api/v1/letterings/search?q=PageTester", 100, false, true),
        ("/api/v1/cities", 500, false, true),
        ("/api/v1/collections", 100, false, true),
        (collection.as_str(), 200, false, true),
        ("/api/v1/contributors/PageTester", 100, false, true),
        ("/api/v1/me/letterings", 100, false, true),
        ("/api/v1/me/notifications", 100, false, true),
        ("/api/v1/admin/moderation", 200, true, true),
        ("/api/v1/admin/comments", 200, true, true),
        ("/api/v1/admin/audit-logs", 200, true, true),
        ("/api/v1/admin/region-policies", 500, true, true),
        ("/api/v1/geo/markers", 10000, false, false),
        ("/api/v1/geo/coverage", 20000, false, false),
        ("/api/v1/admin/location-repairs", 100, true, false),
        (history.as_str(), 100, true, false),
    ];
    for (endpoint, maximum, admin, offset) in endpoints {
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        for limit in [-1, 0, maximum + 1] {
            let path = format!("{endpoint}{separator}limit={limit}");
            assert_eq!(
                fixture.response(&path, admin).await.status(),
                StatusCode::BAD_REQUEST,
                "{path}"
            );
        }
        if offset {
            for invalid in [-1, 10001] {
                let path = format!("{endpoint}{separator}offset={invalid}");
                assert_eq!(
                    fixture.response(&path, admin).await.status(),
                    StatusCode::BAD_REQUEST,
                    "{path}"
                );
            }
        }
        let response = fixture
            .get(&format!("{endpoint}{separator}limit={maximum}"), admin)
            .await;
        if let Some(limit) = response.get("limit") {
            assert_eq!(limit.as_i64(), Some(maximum));
        }
        let legacy = fixture.response(endpoint, admin).await;
        assert_eq!(
            legacy.status(),
            StatusCode::OK,
            "existing client request: {endpoint}"
        );
    }
}

#[sqlx::test]
async fn equal_timestamps_have_consistent_order_across_discovery_owner_and_admin_pages(
    pool: PgPool,
) {
    let fixture = Fixture::new(&pool).await;
    let descending: Vec<_> = fixture.ids.iter().rev().copied().collect();
    let collection = format!("/api/v1/collections/{}", fixture.collection);
    for (endpoint, field, admin, expected) in [
        (
            "/api/v1/letterings",
            Some("letterings"),
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/letterings?sort_by=oldest",
            Some("letterings"),
            false,
            fixture.ids.as_slice(),
        ),
        (
            "/api/v1/letterings?sort_by=popular",
            Some("letterings"),
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/letterings/search?q=PageTester",
            None,
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/contributors/PageTester",
            Some("letterings"),
            false,
            descending.as_slice(),
        ),
        (
            collection.as_str(),
            Some("items"),
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/me/letterings",
            Some("items"),
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/me/notifications",
            Some("items"),
            false,
            descending.as_slice(),
        ),
        (
            "/api/v1/admin/moderation",
            Some("items"),
            true,
            descending.as_slice(),
        ),
        (
            "/api/v1/admin/moderation?status=APPROVED",
            Some("items"),
            true,
            fixture.ids.as_slice(),
        ),
        (
            "/api/v1/admin/audit-logs?action=PAGE_TEST",
            Some("items"),
            true,
            descending.as_slice(),
        ),
    ] {
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        let mut found = Vec::new();
        for offset in [0, 3, 6] {
            let response = fixture
                .get(
                    &format!("{endpoint}{separator}limit=3&offset={offset}"),
                    admin,
                )
                .await;
            found.extend(ids(&response, field));
        }
        assert_eq!(found, expected, "{endpoint}");
    }
}

#[sqlx::test]
async fn configured_offset_budget_reaches_the_http_boundary(pool: PgPool) {
    let app = spawn_app_with_config(&pool, |config| config.limits.pagination.max_offset = 1).await;
    for (offset, expected) in [(1, StatusCode::OK), (2, StatusCode::BAD_REQUEST)] {
        let response = send(
            &app.app,
            Request::builder()
                .uri(format!("/api/v1/letterings?offset={offset}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), expected);
    }
}
