use super::helpers::{
    assert_status, expect_status, multipart_upload_body, read_json, read_text, send, spawn_app,
    tiny_png_bytes, unique_email,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};

const DEFAULT_CITY_ID: &str = "0194f123-4567-7abc-8def-0123456789ab";

async fn register_user_and_token(app: &Router) -> String {
    let email = unique_email("upload-it");
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": "StrongerPass123!",
                "display_name": "Upload Tester"
            })
            .to_string(),
        ))
        .expect("failed to build register request");

    let res = expect_status(send(app, req).await, StatusCode::OK).await;
    let body: Value = read_json(res).await;
    body["token"]
        .as_str()
        .expect("token missing in register response")
        .to_string()
}

async fn upload_for_user(app: &Router, token: &str) -> Value {
    let (boundary, body) = multipart_upload_body(
        "Uploader007",
        "560001",
        "Initial upload context",
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

    let res = expect_status(send(app, req).await, StatusCode::OK).await;
    read_json::<Value>(res).await
}

#[tokio::test]
async fn upload_rejects_request_without_image_part() {
    let app = spawn_app().await;

    let boundary = "----ttl-boundary-no-image";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"contributor_tag\"\r\n\r\nNoImageUser\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"pin_code\"\r\n\r\n560001\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"city_id\"\r\n\r\n{city}\r\n\
         --{b}--\r\n",
        b = boundary,
        city = DEFAULT_CITY_ID
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .expect("failed to build request");

    let res = send(&app.app, req).await;
    assert_status(res.status(), StatusCode::BAD_REQUEST);
    let text = read_text(res).await;
    assert!(
        text.contains("Missing image"),
        "expected missing image error, got: {text}"
    );
}

#[tokio::test]
async fn authenticated_upload_is_visible_in_my_uploads() {
    let app = spawn_app().await;
    let token = register_user_and_token(&app.app).await;
    let uploaded = upload_for_user(&app.app, &token).await;
    let uploaded_id = uploaded["id"]
        .as_str()
        .expect("upload id missing")
        .to_string();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/me/letterings?limit=10&offset=0")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .expect("failed to build my uploads request");

    let res = expect_status(send(&app.app, req).await, StatusCode::OK).await;
    let payload: Value = read_json(res).await;
    let items = payload["items"]
        .as_array()
        .expect("items must be an array in my uploads response");
    assert!(
        items.iter().any(|item| item["id"] == uploaded_id),
        "uploaded item {uploaded_id} should appear in user uploads"
    );
}

#[tokio::test]
async fn updating_my_upload_creates_metadata_history_entries() {
    let app = spawn_app().await;
    let token = register_user_and_token(&app.app).await;
    let uploaded = upload_for_user(&app.app, &token).await;
    let uploaded_id = uploaded["id"].as_str().expect("upload id missing");

    let update_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/me/letterings/{}", uploaded_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "description": "Updated history-aware description",
                "contributor_tag": "Uploader008",
                "pin_code": "560002"
            })
            .to_string(),
        ))
        .expect("failed to build metadata update request");

    let update_res = send(&app.app, update_req).await;
    assert_status(update_res.status(), StatusCode::OK);
    let updated: Value = read_json(update_res).await;
    assert_eq!(updated["contributor_tag"], "Uploader008");
    assert_eq!(updated["pin_code"], "560002");
    assert_eq!(updated["description"], "Updated history-aware description");

    let timeline_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/me/letterings/{}/timeline", uploaded_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .expect("failed to build timeline request");

    let timeline_res = send(&app.app, timeline_req).await;
    assert_status(timeline_res.status(), StatusCode::OK);
    let timeline: Value = read_json(timeline_res).await;

    let metadata_history = timeline["metadata_history"]
        .as_array()
        .expect("metadata_history should be an array");
    assert!(
        metadata_history.len() >= 3,
        "expected at least three metadata history records"
    );

    let status_history = timeline["status_history"]
        .as_array()
        .expect("status_history should be an array");
    assert!(
        !status_history.is_empty(),
        "expected at least one status history record"
    );
}
