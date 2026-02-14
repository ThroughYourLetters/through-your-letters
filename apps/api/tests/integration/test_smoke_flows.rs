use super::helpers::{
    assert_status, expect_status, multipart_upload_body, read_json, send, spawn_app,
    tiny_png_bytes, unique_email,
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};

const DEFAULT_CITY_ID: &str = "0194f123-4567-7abc-8def-0123456789ab";

#[tokio::test]
async fn smoke_auth_upload_discover_comment_and_admin_comment_moderation() {
    let app = spawn_app().await;

    let register_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": unique_email("smoke"),
                "password": "StrongSmokePass123!",
                "display_name": "Smoke User"
            })
            .to_string(),
        ))
        .expect("failed to build register request");
    let register_res = expect_status(send(&app.app, register_req).await, StatusCode::OK).await;
    let register_body: Value = read_json(register_res).await;
    let user_token = register_body["token"].as_str().expect("missing user token");

    let (boundary, upload_body) = multipart_upload_body(
        "SmokeUserTag",
        "560301",
        "Smoke upload",
        DEFAULT_CITY_ID,
        &tiny_png_bytes(),
    );
    let upload_req = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .header(header::AUTHORIZATION, format!("Bearer {}", user_token))
        .body(Body::from(upload_body))
        .expect("failed to build upload request");
    let upload_res = expect_status(send(&app.app, upload_req).await, StatusCode::OK).await;
    let upload_payload: Value = read_json(upload_res).await;
    let lettering_id = upload_payload["id"]
        .as_str()
        .expect("missing lettering id in upload response");

    let gallery_req = Request::builder()
        .method("GET")
        .uri("/api/v1/letterings?limit=20&offset=0")
        .body(Body::empty())
        .expect("failed to build gallery request");
    let gallery_res = expect_status(send(&app.app, gallery_req).await, StatusCode::OK).await;
    let gallery_payload: Value = read_json(gallery_res).await;
    let gallery_items = gallery_payload["letterings"]
        .as_array()
        .expect("gallery letterings should be an array");
    assert!(
        gallery_items.iter().any(|item| item["id"] == lettering_id),
        "uploaded lettering should be discoverable"
    );

    let add_comment_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/letterings/{}/comments", lettering_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", user_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "content": "This is a clean smoke comment." }).to_string(),
        ))
        .expect("failed to build add-comment request");
    let add_comment_res =
        expect_status(send(&app.app, add_comment_req).await, StatusCode::OK).await;
    let comment_payload: Value = read_json(add_comment_res).await;
    let comment_id = comment_payload["id"]
        .as_str()
        .expect("missing comment id in add comment response");

    let admin_login_req = Request::builder()
        .method("POST")
        .uri("/api/v1/admin/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": app.admin_email,
                "password": app.admin_password
            })
            .to_string(),
        ))
        .expect("failed to build admin login request");
    let admin_login_res =
        expect_status(send(&app.app, admin_login_req).await, StatusCode::OK).await;
    let admin_login_payload: Value = read_json(admin_login_res).await;
    let admin_token = admin_login_payload["token"]
        .as_str()
        .expect("missing admin token");

    let hide_comment_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/admin/comments/{}/hide", comment_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "reason": "Smoke moderation hide check" }).to_string(),
        ))
        .expect("failed to build hide-comment request");
    let _hide_comment_res =
        expect_status(send(&app.app, hide_comment_req).await, StatusCode::OK).await;

    let get_comments_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/letterings/{}/comments", lettering_id))
        .body(Body::empty())
        .expect("failed to build get-comments request");
    let get_comments_res = send(&app.app, get_comments_req).await;
    assert_status(get_comments_res.status(), StatusCode::OK);
    let comments_after_hide: Value = read_json(get_comments_res).await;
    let comments_after_hide = comments_after_hide
        .as_array()
        .expect("comments response should be an array");
    assert!(
        comments_after_hide
            .iter()
            .all(|comment| comment["id"] != comment_id),
        "hidden comment should not be returned in visible comments list"
    );

    let restore_comment_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/admin/comments/{}/restore", comment_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .body(Body::empty())
        .expect("failed to build restore-comment request");
    let _restore_comment_res =
        expect_status(send(&app.app, restore_comment_req).await, StatusCode::OK).await;

    let get_comments_after_restore_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/letterings/{}/comments", lettering_id))
        .body(Body::empty())
        .expect("failed to build get-comments-after-restore request");
    let get_comments_after_restore_res = send(&app.app, get_comments_after_restore_req).await;
    assert_status(get_comments_after_restore_res.status(), StatusCode::OK);
    let comments_after_restore: Value = read_json(get_comments_after_restore_res).await;
    let comments_after_restore = comments_after_restore
        .as_array()
        .expect("comments response should be an array");
    assert!(
        comments_after_restore
            .iter()
            .any(|comment| comment["id"] == comment_id),
        "restored comment should be returned in visible comments list"
    );
}
