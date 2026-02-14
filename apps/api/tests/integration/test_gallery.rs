use super::helpers::{
    assert_status, multipart_upload_body, read_json, send, spawn_app, tiny_png_bytes, unique_email,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};

const DEFAULT_CITY_ID: &str = "0194f123-4567-7abc-8def-0123456789ab";

async fn register_user_and_token(app: &Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": unique_email("gallery-it"),
                "password": "StrongerPass123!",
                "display_name": "Gallery Tester"
            })
            .to_string(),
        ))
        .expect("failed to build register request");

    let res = send(app, req).await;
    assert_status(res.status(), StatusCode::OK);
    let body: Value = read_json(res).await;
    body["token"]
        .as_str()
        .expect("missing token in register response")
        .to_string()
}

async fn upload_artifact(app: &Router, token: &str, contributor: &str, pin_code: &str) -> String {
    let (boundary, body) = multipart_upload_body(
        contributor,
        pin_code,
        "Gallery integration artifact",
        DEFAULT_CITY_ID,
        &tiny_png_bytes(),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::from(body))
        .expect("failed to build upload request");

    let res = send(app, req).await;
    assert_status(res.status(), StatusCode::OK);
    let payload: Value = read_json(res).await;
    payload["id"]
        .as_str()
        .expect("upload response missing id")
        .to_string()
}

#[tokio::test]
async fn gallery_returns_paginated_items_with_total() {
    let app = spawn_app().await;
    let token = register_user_and_token(&app.app).await;
    let _ = upload_artifact(&app.app, &token, "GalleryA", "560101").await;
    let _ = upload_artifact(&app.app, &token, "GalleryB", "560102").await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/letterings?limit=1&offset=0")
        .body(Body::empty())
        .expect("failed to build gallery request");

    let res = send(&app.app, req).await;
    assert_status(res.status(), StatusCode::OK);
    let payload: Value = read_json(res).await;

    assert_eq!(payload["limit"].as_i64(), Some(1));
    assert_eq!(payload["offset"].as_i64(), Some(0));
    assert!(
        payload["total"].as_i64().unwrap_or(0) >= 2,
        "gallery total should include uploaded entries"
    );
    assert_eq!(payload["letterings"].as_array().map(|v| v.len()), Some(1));
}

#[tokio::test]
async fn gallery_sort_oldest_returns_ascending_created_at_order() {
    let app = spawn_app().await;
    let token = register_user_and_token(&app.app).await;
    let _ = upload_artifact(&app.app, &token, "SortOldA", "560201").await;
    sleep(Duration::from_millis(10)).await;
    let _ = upload_artifact(&app.app, &token, "SortOldB", "560202").await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/letterings?sort_by=oldest&limit=10&offset=0")
        .body(Body::empty())
        .expect("failed to build oldest-sort request");

    let res = send(&app.app, req).await;
    assert_status(res.status(), StatusCode::OK);
    let payload: Value = read_json(res).await;
    let letterings = payload["letterings"]
        .as_array()
        .expect("letterings should be an array");

    assert!(
        letterings.len() >= 2,
        "expected at least two records for oldest sort check"
    );

    let first_created = letterings[0]["created_at"]
        .as_str()
        .expect("created_at missing on first result");
    let second_created = letterings[1]["created_at"]
        .as_str()
        .expect("created_at missing on second result");

    assert!(
        first_created <= second_created,
        "expected ascending created_at for oldest sort, got {first_created} then {second_created}"
    );
}
