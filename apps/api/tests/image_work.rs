#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use api::infrastructure::{blocking::BlockingError, ml::image_analyzer::ImageAnalyzer};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use futures_util::StreamExt;
use helpers::{expect_status, send, spawn_app_with_config, tiny_png_bytes};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::{io::Cursor, num::NonZeroUsize, sync::atomic::Ordering, time::Duration};

fn upload(parts: Vec<(&str, Vec<u8>)>) -> Request<Body> {
    let boundary = "ttl-image-work-test";
    let mut body = Vec::new();
    for (name, value) in parts {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(&value);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let chunks = body
        .chunks(37)
        .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk)))
        .collect::<Vec<_>>();
    Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from_stream(futures_util::stream::iter(chunks)))
        .unwrap()
}

fn fields(image: Vec<u8>) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("contributor_tag", b"Bounded upload".to_vec()),
        ("pin_code", b"560001".to_vec()),
        ("description", b"Image test".to_vec()),
        ("city_id", b"0194f123-4567-7abc-8def-0123456789ab".to_vec()),
        ("image", image),
    ]
}

fn bmp_header(width: i32, height: i32) -> Vec<u8> {
    let mut bytes = vec![0; 54];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&54_u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    bytes[22..26].copy_from_slice(&height.to_le_bytes());
    bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
    bytes
}

#[sqlx::test]
async fn huge_dimensions_and_pixel_counts_are_rejected_before_storage_or_database_writes(
    pool: sqlx::PgPool,
) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    for bytes in [bmp_header(50000, 50000), bmp_header(7000, 5000)] {
        expect_status(
            send(&app.app, upload(fields(bytes))).await,
            StatusCode::PAYLOAD_TOO_LARGE,
        )
        .await;
    }
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM letterings")
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analysis_jobs")
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
async fn decoded_byte_limit_rejects_a_small_compressed_image(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.images.max_decoded_bytes = 1024
    })
    .await;
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::new(32, 32))
        .write_to(&mut png, ImageFormat::Png)
        .unwrap();
    assert!(png.get_ref().len() < 1024);
    for bytes in [png.into_inner(), bmp_header(32, 32)] {
        expect_status(
            send(&app.app, upload(fields(bytes))).await,
            StatusCode::PAYLOAD_TOO_LARGE,
        )
        .await;
    }
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
}

#[sqlx::test]
async fn chunked_multipart_enforces_file_text_field_count_and_duplicate_limits(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.images.max_input_bytes = 1024;
        config.limits.images.max_field_bytes = 64;
        config.limits.images.max_fields = 8;
    })
    .await;
    let mut oversized_text = fields(tiny_png_bytes());
    oversized_text[2].1 = vec![b'a'; 65];
    let mut too_many_fields = fields(tiny_png_bytes());
    too_many_fields.extend((0..4).map(|_| ("extra", b"value".to_vec())));
    let mut duplicate = fields(tiny_png_bytes());
    duplicate.push(("image", tiny_png_bytes()));
    let mut bad_utf8 = fields(tiny_png_bytes());
    bad_utf8[2].1 = vec![0xff];
    for (parts, status) in [
        (fields(vec![0; 1025]), StatusCode::PAYLOAD_TOO_LARGE),
        (oversized_text, StatusCode::PAYLOAD_TOO_LARGE),
        (too_many_fields, StatusCode::PAYLOAD_TOO_LARGE),
        (duplicate, StatusCode::BAD_REQUEST),
        (bad_utf8, StatusCode::BAD_REQUEST),
    ] {
        expect_status(send(&app.app, upload(parts)).await, status).await;
    }
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
    let mut exactly_at_limit = tiny_png_bytes();
    exactly_at_limit.resize(1024, 0);
    expect_status(
        send(&app.app, upload(fields(exactly_at_limit))).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 2);
}

#[sqlx::test]
async fn slow_uploads_hold_admission_until_the_deadline_and_leave_health_available(
    pool: sqlx::PgPool,
) {
    let app = spawn_app_with_config(&pool, |config| {
        config.limits.images.concurrent_uploads = NonZeroUsize::new(1).unwrap();
        config.limits.images.work_timeout_ms = 1000;
        config.limits.images.upload_timeout_ms = 2000;
        config.limits.images.scan_timeout_ms = 1000;
    })
    .await;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let body = futures_util::stream::once(async {
        started_tx.send(()).unwrap();
        Ok::<_, std::io::Error>(Bytes::from_static(b"--pending\r\n"))
    })
    .chain(futures_util::stream::pending());
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/letterings/upload")
        .header("content-type", "multipart/form-data; boundary=pending")
        .body(Body::from_stream(body))
        .unwrap();
    let pending = tokio::spawn({
        let router = app.app.clone();
        async move { send(&router, request).await }
    });
    tokio::time::timeout(Duration::from_secs(2), started_rx)
        .await
        .unwrap()
        .unwrap();
    expect_status(
        send(&app.app, upload(fields(tiny_png_bytes()))).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    let health = tokio::time::timeout(
        Duration::from_secs(1),
        send(
            &app.app,
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    expect_status(health, StatusCode::OK).await;
    expect_status(
        tokio::time::timeout(Duration::from_secs(4), pending)
            .await
            .unwrap()
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
    let mut missing_image = fields(tiny_png_bytes());
    missing_image.pop();
    expect_status(
        send(&app.app, upload(missing_image)).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
}

#[sqlx::test]
async fn uploads_and_model_calls_share_capacity_after_caller_cancellation(pool: sqlx::PgPool) {
    let app = spawn_app_with_config(&pool, |_| {}).await;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let work = tokio::spawn({
        let images = app.images.clone();
        async move {
            images
                .run(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                })
                .await
        }
    });
    started_rx.await.unwrap();
    work.abort();
    assert!(work.await.unwrap_err().is_cancelled());
    let detector = ImageAnalyzer::new("unused", false, app.images.clone()).unwrap();
    let error = detector
        .extract_colors(tiny_png_bytes().into())
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<BlockingError>(),
        Some(BlockingError::Busy)
    ));
    expect_status(
        send(&app.app, upload(fields(tiny_png_bytes()))).await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    assert_eq!(app.storage_uploads.load(Ordering::SeqCst), 0);
    expect_status(
        send(
            &app.app,
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while app.images.run(|| ()).await.is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    expect_status(
        send(&app.app, upload(fields(tiny_png_bytes()))).await,
        StatusCode::OK,
    )
    .await;
    assert!(
        !detector
            .extract_colors(tiny_png_bytes().into())
            .await
            .unwrap()
            .is_empty()
    );
}
