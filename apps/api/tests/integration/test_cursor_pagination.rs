use super::{
    helpers::{read_json, send},
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeZone, Utc};
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
fn ids(value: &Value) -> Vec<Uuid> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().parse().unwrap())
        .collect()
}
async fn array_page(f: &Fixture, path: &str) -> (Value, Option<String>) {
    let response = f.response(path, false).await;
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    let next = response.headers().get("link").map(|value| {
        let value = value.to_str().unwrap();
        assert!(value.ends_with("; rel=\"next\""));
        value
            .strip_prefix('<')
            .unwrap()
            .split_once('>')
            .unwrap()
            .0
            .to_owned()
    });
    (read_json(response).await, next)
}

#[sqlx::test]
async fn gallery_continues_without_duplicates_after_new_approvals_and_cursor_row_deletion(
    pool: PgPool,
) {
    let f = Fixture::new(&pool).await;
    let first = f.get("/api/v1/letterings?limit=3", false).await;
    let cursor = first["next_cursor"].as_str().unwrap();
    let mut found = ids(&first["letterings"]);
    let new_id = Uuid::from_u128(20000);
    sqlx::query("INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status,created_at) SELECT $1,city_id,user_id,contributor_tag,'image','small','medium','large',pin_code,'PENDING',created_at + INTERVAL '1 day' FROM letterings WHERE id=$2")
        .bind(new_id).bind(f.ids[0]).execute(&pool).await.unwrap();
    let approved = send(
        &f.app.app,
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/admin/letterings/{new_id}/approve"))
            .header("authorization", format!("Bearer {}", f.admin))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(approved.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        f.get("/api/v1/letterings?limit=3", false).await["letterings"][0]["id"],
        new_id.to_string()
    );
    sqlx::query("DELETE FROM letterings WHERE id=$1")
        .bind(*found.last().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let mut next = Some(cursor.to_owned());
    while let Some(cursor) = next {
        let page = f
            .get(&after("/api/v1/letterings?limit=3", &cursor), false)
            .await;
        found.extend(ids(&page["letterings"]));
        next = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(found, f.ids.iter().rev().copied().collect::<Vec<_>>());
    for invalid in [
        after("/api/v1/letterings?limit=3&sort_by=oldest", cursor),
        after("/api/v1/letterings?limit=3&style=another-style", cursor),
        after("/api/v1/letterings?limit=3&offset=1", cursor),
        after("/api/v1/letterings", "not a cursor"),
        after("/api/v1/letterings", &"x".repeat(2049)),
    ] {
        assert_eq!(
            f.response(&invalid, false).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    for sort in ["oldest", "popular"] {
        let path = format!("/api/v1/letterings?limit=2&sort_by={sort}");
        let mut page = f.get(&path, false).await;
        let mut found = ids(&page["letterings"]);
        while let Some(cursor) = page["next_cursor"].as_str() {
            page = f.get(&after(&path, cursor), false).await;
            found.extend(ids(&page["letterings"]));
        }
        let unique: std::collections::HashSet<_> = found.iter().collect();
        assert_eq!(unique.len(), found.len());
        assert_eq!(found.len(), 8);
    }
}

#[sqlx::test]
async fn comments_and_challenges_keep_array_bodies_and_continue_after_anchor_deletion(
    pool: PgPool,
) {
    let f = Fixture::new(&pool).await;
    let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    sqlx::query("DELETE FROM challenges")
        .execute(&pool)
        .await
        .unwrap();
    for index in 0..8_u128 {
        sqlx::query("INSERT INTO comments(id,lettering_id,user_id,content,status,created_at) SELECT $1,id,user_id,'A public comment','VISIBLE',$3 FROM letterings WHERE id=$2")
            .bind(Uuid::from_u128(4000+index)).bind(f.ids[0]).bind(time).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO challenges(id,title,status,created_at) VALUES($1,'Page challenge','ACTIVE',$2)")
            .bind(Uuid::from_u128(5000+index)).bind(time).execute(&pool).await.unwrap();
    }
    for endpoint in [
        format!("/api/v1/letterings/{}/comments", f.ids[0]),
        format!("/api/v1/letterings/{}/revisits", f.ids[0]),
        "/api/v1/challenges".into(),
    ] {
        for limit in [-1, 0, 101] {
            assert_eq!(
                f.response(&format!("{endpoint}?limit={limit}"), false)
                    .await
                    .status(),
                StatusCode::BAD_REQUEST,
            );
        }
        assert_eq!(
            f.response(&format!("{endpoint}?limit=100"), false)
                .await
                .status(),
            StatusCode::OK
        );
    }
    for (path, table, start) in [
        (
            format!("/api/v1/letterings/{}/comments?limit=3", f.ids[0]),
            "comments",
            4000_u128,
        ),
        ("/api/v1/challenges?limit=3".into(), "challenges", 5000_u128),
    ] {
        let (first, mut next) = array_page(&f, &path).await;
        assert_eq!(first.as_array().unwrap().len(), 3);
        let mut found = ids(&first);
        sqlx::query(&format!("DELETE FROM {table} WHERE id=$1"))
            .bind(*found.last().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        while let Some(path) = next {
            let (page, following) = array_page(&f, &path).await;
            assert!(page.as_array().unwrap().len() <= 3);
            found.extend(ids(&page));
            next = following;
        }
        assert_eq!(
            found,
            (start..start + 8)
                .rev()
                .map(Uuid::from_u128)
                .collect::<Vec<_>>()
        );
        let legacy = f.get(path.split('?').next().unwrap(), false).await;
        assert!(legacy.is_array());
    }
    let first = f.get("/api/v1/letterings?limit=2", false).await;
    let wrong = after(
        &format!("/api/v1/letterings/{}/comments", f.ids[0]),
        first["next_cursor"].as_str().unwrap(),
    );
    assert_eq!(
        f.response(&wrong, false).await.status(),
        StatusCode::BAD_REQUEST
    );
    let (_, next) = array_page(
        &f,
        &format!("/api/v1/letterings/{}/comments?limit=2", f.ids[0]),
    )
    .await;
    let cursor = reqwest::Url::parse("http://fixture.test")
        .unwrap()
        .join(&next.unwrap())
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "after")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        f.response(
            &after(
                &format!("/api/v1/letterings/{}/comments", f.ids[1]),
                &cursor
            ),
            false
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test]
async fn nearby_and_revisit_pages_are_bounded_and_validate_their_scope(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    sqlx::query("UPDATE letterings SET location=ST_SetSRID(ST_MakePoint(77.5,12.9),4326)::geography,location_source='postal_code'").execute(&pool).await.unwrap();
    let time = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for (index, &id) in f.ids.iter().skip(1).enumerate() {
        sqlx::query("INSERT INTO location_revisits(id,original_lettering_id,revisit_lettering_id,created_at) VALUES($1,$2,$3,$4)")
            .bind(Uuid::from_u128(3000+index as u128)).bind(f.ids[0]).bind(id).bind(time).execute(&pool).await.unwrap();
    }
    let nearby = "/api/v1/geo/nearby?lat=12.9&lng=77.5&radius_m=1000&limit=3";
    let (first, mut next) = array_page(&f, nearby).await;
    let mut found = ids(&first);
    while let Some(path) = next {
        let (page, following) = array_page(&f, &path).await;
        found.extend(ids(&page));
        next = following;
    }
    assert_eq!(found, f.ids.iter().rev().copied().collect::<Vec<_>>());
    let (_, next) = array_page(&f, nearby).await;
    let mut changed = reqwest::Url::parse("http://fixture.test")
        .unwrap()
        .join(&next.unwrap())
        .unwrap();
    let cursor = changed
        .query_pairs()
        .find(|(key, _)| key == "after")
        .unwrap()
        .1
        .into_owned();
    changed.set_query(Some("lat=13&lng=77.5&radius_m=1000"));
    assert_eq!(
        f.response(
            &after(
                &format!("{}?{}", changed.path(), changed.query().unwrap()),
                &cursor
            ),
            false
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    for params in [
        "lat=NaN&lng=77&radius_m=1",
        "lat=91&lng=77&radius_m=1",
        "lat=12&lng=181&radius_m=1",
        "lat=12&lng=77&radius_m=-1",
        "lat=12&lng=77&radius_m=1000&limit=10001",
    ] {
        assert_eq!(
            f.response(&format!("/api/v1/geo/nearby?{params}"), false)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let path = format!("/api/v1/letterings/{}/revisits?limit=2", f.ids[0]);
    let mut page = f.get(&path, false).await;
    let mut links = ids(&page["revisits"]);
    sqlx::query("DELETE FROM location_revisits WHERE id=$1")
        .bind(*links.last().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    while let Some(cursor) = page["next_cursor"].as_str() {
        page = f.get(&after(&path, cursor), false).await;
        links.extend(ids(&page["revisits"]));
    }
    assert_eq!(
        links,
        (3000..3007).rev().map(Uuid::from_u128).collect::<Vec<_>>()
    );
}
