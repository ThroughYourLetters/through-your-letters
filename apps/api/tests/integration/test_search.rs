use super::{
    helpers::{read_json, send},
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

fn search(query: &str, language: &str) -> String {
    let mut url = reqwest::Url::parse("http://fixture.test/api/v1/letterings/search").unwrap();
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("lang", language);
    format!("{}?{}", url.path(), url.query().unwrap())
}
fn ids(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn search_matches_indexed_languages_substrings_and_edits_without_exposing_hidden_uploads(
    pool: PgPool,
) {
    let f = Fixture::new(&pool).await;
    sqlx::query(
        "UPDATE letterings SET detected_text='Lantern painted by local hands' WHERE id=ANY($1)",
    )
    .bind(vec![f.ids[0], f.ids[4], f.ids[5]])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE letterings SET detected_text='ಕನ್ನಡ ಹಳೆಯ ಅಂಗಡಿ' WHERE id=$1")
        .bind(f.ids[1])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET description='A faded riverbank capture' WHERE id=$1")
        .bind(f.ids[2])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET contributor_tag='RareWalker' WHERE id=$1")
        .bind(f.ids[3])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[4])
        .execute(&pool)
        .await
        .unwrap();
    let blocked_city = Uuid::now_v7();
    sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,'Search policy city','US')")
        .bind(blocked_city)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO region_policies(country_code,discoverability_enabled) VALUES('US',false) ON CONFLICT(country_code) DO UPDATE SET discoverability_enabled=false")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE letterings SET city_id=$1 WHERE id=$2")
        .bind(blocked_city)
        .bind(f.ids[5])
        .execute(&pool)
        .await
        .unwrap();
    for (query, language, expected) in [
        ("painting lanterns", "en-US", f.ids[0]),
        ("painted lantern", "kn", f.ids[0]),
        ("ಕನ್ನಡ ಅಂಗಡಿ", "kn", f.ids[1]),
        ("riverba", "en", f.ids[2]),
        ("RareWalk", "en", f.ids[3]),
    ] {
        assert_eq!(
            ids(&f.get(&search(query, language), false).await),
            vec![expected.to_string()],
            "{query} / {language}"
        );
    }
    let updated = send(
        &f.app.app,
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/me/letterings/{}", f.ids[0]))
            .header("authorization", format!("Bearer {}", f.token))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"description":"A gold leaf artisan worked here"}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        ids(&f.get(&search("artisan gold", "kn"), false).await),
        vec![f.ids[0].to_string()]
    );
    let liked = send(
        &f.app.app,
        Request::builder()
            .method("POST")
            .uri(format!("/api/v1/letterings/{}/like", f.ids[0]))
            .header("authorization", format!("Bearer {}", f.token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(liked.status(), StatusCode::OK);
    assert_eq!(read_json::<Value>(liked).await["likes_count"], 1);
    assert_eq!(
        ids(&f.get(&search("painting lanterns", "en"), false).await),
        vec![f.ids[0].to_string()]
    );
    assert_eq!(
        ids(&f.get(&search("artisan gold", "kn"), false).await),
        vec![f.ids[0].to_string()]
    );
}
