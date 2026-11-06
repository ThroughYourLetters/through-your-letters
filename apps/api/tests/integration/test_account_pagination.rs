use super::{
    helpers::{read_json, send},
    test_pagination::Fixture,
};
use api::presentation::http::middleware::user::UserClaims;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeZone, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

fn after(path: &str, cursor: &str) -> String {
    let mut url = reqwest::Url::parse("http://fixture.test")
        .unwrap()
        .join(path)
        .unwrap();
    url.query_pairs_mut().append_pair("after", cursor);
    format!("{}?{}", url.path(), url.query().unwrap())
}
fn ids(rows: &Value) -> Vec<Uuid> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().parse().unwrap())
        .collect()
}
async fn other_token(pool: &PgPool) -> String {
    let id = Uuid::now_v7();
    let email = format!("{id}@example.test");
    sqlx::query("INSERT INTO users(id,email,password_hash) VALUES($1,$2,'test')")
        .bind(id)
        .bind(&email)
        .execute(pool)
        .await
        .unwrap();
    encode(
        &Header::default(),
        &UserClaims {
            sub: id.to_string(),
            email,
            role: "USER".into(),
            exp: (Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            session_version: 0,
        },
        &EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap()
}
async fn with_token(f: &Fixture, path: &str, token: &str) -> axum::response::Response {
    send(
        &f.app.app,
        Request::builder()
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

#[sqlx::test]
async fn account_cursors_survive_new_rows_and_deleted_anchors_and_cannot_change_owner(
    pool: PgPool,
) {
    let f = Fixture::new(&pool).await;
    let other = other_token(&pool).await;
    for (endpoint, table) in [
        ("/api/v1/me/letterings", "letterings"),
        ("/api/v1/me/notifications", "notifications"),
    ] {
        let path = format!("{endpoint}?limit=3");
        let mut page = f.get(&path, false).await;
        let first_cursor = page["next_cursor"].as_str().unwrap().to_owned();
        let mut found = ids(&page["items"]);
        let new_id = Uuid::now_v7();
        if table == "letterings" {
            sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) SELECT $1,city_id,user_id,'PageTester','image','small','medium','large','560001','APPROVED' FROM letterings WHERE id=$2")
                .bind(new_id).bind(f.ids[0]).execute(&pool).await.unwrap();
        } else {
            sqlx::query("INSERT INTO notifications(id,user_id,type,title) SELECT $1,user_id,'test','New notice' FROM notifications WHERE id=$2")
                .bind(new_id).bind(f.ids[0]).execute(&pool).await.unwrap();
        }
        sqlx::query(&format!("DELETE FROM {table} WHERE id=$1"))
            .bind(*found.last().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        while let Some(cursor) = page["next_cursor"].as_str() {
            page = f.get(&after(&path, cursor), false).await;
            assert!(page["items"].as_array().unwrap().len() <= 3);
            found.extend(ids(&page["items"]));
        }
        assert_eq!(found, f.ids.iter().rev().copied().collect::<Vec<_>>());
        assert_eq!(
            with_token(&f, &after(&path, &first_cursor), &other)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        for query in ["limit=0", "limit=-1", "limit=101"] {
            assert_eq!(
                f.response(&format!("{endpoint}?{query}"), false)
                    .await
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            f.response(&format!("{endpoint}?limit=100"), false)
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            f.response(
                &after(&format!("{endpoint}?offset=1"), &first_cursor),
                false
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        let legacy = f.get(endpoint, false).await;
        assert_eq!(legacy["limit"], 20);
        assert!(legacy["items"].is_array());
        if table == "letterings" {
            assert_eq!(
                f.response(
                    &after("/api/v1/me/letterings?status=PENDING", &first_cursor),
                    false
                )
                .await
                .status(),
                StatusCode::BAD_REQUEST
            );
            // Keep the second independent list's fixture intact.
            sqlx::query("DELETE FROM letterings WHERE id=$1")
                .bind(new_id)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
}

#[sqlx::test]
async fn timeline_streams_continue_independently_and_keep_ownership_checks(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    sqlx::query("DELETE FROM lettering_status_history WHERE lettering_id=$1")
        .bind(f.ids[0])
        .execute(&pool)
        .await
        .unwrap();
    let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for index in 0..8_u128 {
        sqlx::query("INSERT INTO lettering_status_history(id,lettering_id,to_status,actor_type,created_at) VALUES($1,$2,'PENDING','SYSTEM',$3)")
            .bind(Uuid::from_u128(50000+index)).bind(f.ids[0]).bind(time).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO lettering_metadata_history(id,lettering_id,field_name,new_value,created_at) VALUES($1,$2,'description','Revised text',$3)")
            .bind(Uuid::from_u128(60000+index)).bind(f.ids[0]).bind(time).execute(&pool).await.unwrap();
    }
    let endpoint = format!("/api/v1/me/letterings/{}/timeline", f.ids[0]);
    let first = f.get(&format!("{endpoint}?limit=3"), false).await;
    for (section, field, next_field, table, start) in [
        (
            "status",
            "status_history",
            "next_status_cursor",
            "lettering_status_history",
            50000_u128,
        ),
        (
            "metadata",
            "metadata_history",
            "next_metadata_cursor",
            "lettering_metadata_history",
            60000_u128,
        ),
    ] {
        assert_eq!(first[field].as_array().unwrap().len(), 3);
        let cursor = first[next_field].as_str().unwrap();
        let mut found = ids(&first[field]);
        sqlx::query(&format!("DELETE FROM {table} WHERE id=$1"))
            .bind(*found.last().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let mut next = Some(cursor.to_owned());
        while let Some(cursor) = next {
            let page = f
                .get(
                    &after(&format!("{endpoint}?limit=3&section={section}"), &cursor),
                    false,
                )
                .await;
            assert_eq!(
                page[if section == "status" {
                    "metadata_history"
                } else {
                    "status_history"
                }],
                serde_json::json!([])
            );
            found.extend(ids(&page[field]));
            next = page[next_field].as_str().map(str::to_owned);
        }
        assert_eq!(
            found,
            (start..start + 8)
                .rev()
                .map(Uuid::from_u128)
                .collect::<Vec<_>>()
        );
        for invalid in [
            after(&endpoint, cursor),
            after(
                &format!(
                    "{endpoint}?section={}",
                    if section == "status" {
                        "metadata"
                    } else {
                        "status"
                    }
                ),
                cursor,
            ),
            after(
                &format!(
                    "/api/v1/me/letterings/{}/timeline?section={section}",
                    f.ids[1]
                ),
                cursor,
            ),
        ] {
            assert_eq!(
                f.response(&invalid, false).await.status(),
                StatusCode::BAD_REQUEST
            );
        }
        let other = other_token(&pool).await;
        assert_eq!(
            with_token(
                &f,
                &after(&format!("{endpoint}?section={section}"), cursor),
                &other
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
    }
    let legacy = f.get(&endpoint, false).await;
    assert_eq!(legacy["status_history"].as_array().unwrap().len(), 7);
    assert_eq!(legacy["metadata_history"].as_array().unwrap().len(), 7);
    for limit in [0, -1, 101] {
        assert_eq!(
            f.response(&format!("{endpoint}?limit={limit}"), false)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    sqlx::query("INSERT INTO lettering_status_history(id,lettering_id,to_status,actor_type) SELECT gen_random_uuid(),$1,'APPROVED','SYSTEM' FROM generate_series(1,101)").bind(f.ids[0]).execute(&pool).await.unwrap();
    let default_page = f.get(&endpoint, false).await;
    assert_eq!(
        default_page["status_history"].as_array().unwrap().len(),
        100
    );
    assert!(default_page["next_status_cursor"].is_string());
    assert!(default_page["next_metadata_cursor"].is_null());
    let forbidden = with_token(&f, &endpoint, "not-a-session").await;
    assert_eq!(forbidden.status(), StatusCode::UNAUTHORIZED);
    let error: Value = read_json(forbidden).await;
    assert!(!error.to_string().contains("Revised text"));
}
