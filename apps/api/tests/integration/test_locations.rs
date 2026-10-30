use super::helpers::{expect_status, read_json, send, spawn_app};
use api::{
    domain::lettering::{
        entity::*,
        repository::{LetteringRepository, NewUpload},
    },
    infrastructure::repositories::sqlx_lettering_repository::SqlxLetteringRepository,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use uuid::Uuid;

#[sqlx::test]
async fn unresolved_locations_survive_roundtrips_without_map_pins(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let city = Uuid::now_v7();
    sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,$2,'IN')")
        .bind(city)
        .bind(format!("Location fixture {city}"))
        .execute(&app.db)
        .await
        .unwrap();
    let repo = SqlxLetteringRepository::new(app.db.clone());
    let mut unresolved = Lettering {
        id: Uuid::now_v7(),
        city_id: city,
        contributor_tag: "Location fixture".into(),
        image_url: "https://example.test/image".into(),
        pin_code: "560025".into(),
        location: None,
        location_source: LocationSource::Unresolved,
        location_error: Some("no_results".into()),
        ..Default::default()
    };
    repo.create(NewUpload {
        storage_attempt: None,
        request: None,
        lettering: &unresolved,
        owner_id: None,
        analysis_attempts: None,
    })
    .await
    .unwrap();
    unresolved = repo.find_by_id(unresolved.id).await.unwrap().unwrap();
    assert!(unresolved.location.is_none());
    assert_eq!(unresolved.location_source, LocationSource::Unresolved);
    assert_eq!(unresolved.location_error.as_deref(), Some("no_results"));
    unresolved.status = LetteringStatus::Approved;
    repo.update(&unresolved).await.unwrap();

    let postal = Lettering {
        id: Uuid::now_v7(),
        location: Some(Coordinates::new_point(77.61, 12.95)),
        location_source: LocationSource::PostalCode,
        location_error: None,
        ..unresolved.clone()
    };
    repo.create(NewUpload {
        storage_attempt: None,
        request: None,
        lettering: &postal,
        owner_id: None,
        analysis_attempts: None,
    })
    .await
    .unwrap();
    let mut postal = repo.find_by_id(postal.id).await.unwrap().unwrap();
    postal.status = LetteringStatus::Approved;
    let postal = repo.update(&postal).await.unwrap();
    assert_eq!(postal.location.unwrap().coordinates, vec![77.61, 12.95]);
    assert_eq!(postal.location_source, LocationSource::PostalCode);

    for path in [
        format!("/api/v1/geo/markers?city_id={city}"),
        format!("/api/v1/geo/coverage?city_id={city}"),
    ] {
        let response = send(
            &app.app,
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        let response = expect_status(response, StatusCode::OK).await;
        let body: Value = read_json(response).await;
        let points = body.as_array().unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0]["lat"], 12.95);
        assert_eq!(points[0]["lng"], 77.61);
        if points[0].get("id").is_some() {
            assert_eq!(points[0]["location_source"], "postal_code");
        } else {
            assert_eq!(points[0]["count"], 1);
        }
    }
    let response = send(
        &app.app,
        Request::builder()
            .uri(format!("/api/v1/letterings/{}", unresolved.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let response = expect_status(response, StatusCode::OK).await;
    let detail: Value = read_json(response).await;
    assert!(detail["location"].is_null());
    assert_eq!(detail["location_source"], "unresolved");

    let response = send(
        &app.app,
        Request::builder()
            .uri(format!("/api/v1/letterings?city_id={city}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let response = expect_status(response, StatusCode::OK).await;
    let gallery: Value = read_json(response).await;
    let saved = gallery["letterings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == unresolved.id.to_string())
        .unwrap();
    assert!(saved["location"].is_null());
    assert_eq!(saved["location_source"], "unresolved");

    let invalid = Lettering {
        location: Some(Coordinates::new_point(f64::NAN, 12.0)),
        ..unresolved
    };
    assert!(repo.update(&invalid).await.is_err());
    sqlx::query("DELETE FROM letterings WHERE city_id=$1")
        .bind(city)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM cities WHERE id=$1")
        .bind(city)
        .execute(&app.db)
        .await
        .unwrap();
}
