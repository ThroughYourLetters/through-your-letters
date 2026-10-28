use super::helpers::{TestApp, expect_status, read_json, send, spawn_app};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use uuid::Uuid;

async fn get(app: &TestApp, path: &str) -> Value {
    let response = send(
        &app.app,
        Request::builder().uri(path).body(Body::empty()).unwrap(),
    )
    .await;
    read_json(expect_status(response, StatusCode::OK).await).await
}
async fn city(app: &TestApp, country: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO cities(id,name,country_code) VALUES($1,$2,$3)")
        .bind(id)
        .bind(format!("Map fixture {id}"))
        .bind(country)
        .execute(&app.db)
        .await
        .unwrap();
    id
}
async fn photo(
    app: &TestApp,
    city: Uuid,
    point: Option<(f64, f64)>,
    source: &str,
    status: &str,
    script: &str,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status,location_source,location_error,ml_script,ml_style) VALUES($1,$2,'Map fixture','image','small','medium','large',ST_GeogFromText($3),'560025',$4,$5,$6,$7,'Hand-painted')")
        .bind(id).bind(city).bind(point.map(|(lng,lat)|format!("POINT({lng} {lat})"))).bind(status).bind(source).bind(if source=="unresolved" {Some("no_results")} else {None}).bind(script).execute(&app.db).await.unwrap();
    id
}
fn total(data: &Value) -> i64 {
    data["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["count"].as_i64().unwrap())
        .sum()
}

#[tokio::test]
async fn heatmap_filters_exclude_hidden_and_unverified_locations() {
    let app = spawn_app().await;
    let a = city(&app, "IN").await;
    let b = city(&app, "IN").await;
    for _ in 0..2 {
        photo(
            &app,
            a,
            Some((77.61, 12.95)),
            "postal_code",
            "APPROVED",
            "Latin",
        )
        .await;
    }
    photo(
        &app,
        a,
        Some((77.61, 12.95)),
        "postal_code",
        "PENDING",
        "Latin",
    )
    .await;
    photo(
        &app,
        a,
        Some((77.61, 12.95)),
        "postal_code",
        "APPROVED",
        "Devanagari",
    )
    .await;
    photo(
        &app,
        a,
        Some((77.61, 12.95)),
        "legacy_unverified",
        "APPROVED",
        "Latin",
    )
    .await;
    photo(&app, a, None, "unresolved", "APPROVED", "Latin").await;
    photo(
        &app,
        a,
        Some((0.0, 89.0)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    photo(
        &app,
        b,
        Some((77.61, 12.95)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    let query = format!(
        "west=77.60&south=12.94&east=77.62&north=12.96&zoom=15&city_id={a}&script=Latin&style=Hand-painted"
    );
    let density = get(&app, &format!("/api/v1/geo/density?{query}")).await;
    assert_eq!(total(&density), 2);
    assert_eq!(density["truncated"], false);
    let cell = &density["cells"][0];
    let bounds = &cell["bounds"];
    let path = format!(
        "/api/v1/geo/photos?west={}&south={}&east={}&north={}&city_id={a}&script=Latin&style=Hand-painted",
        bounds["west"], bounds["south"], bounds["east"], bounds["north"]
    );
    let photos = get(&app, &path).await;
    assert_eq!(photos["items"].as_array().unwrap().len(), 2);
    let summary = get(
        &app,
        &format!("/api/v1/geo/summary?city_id={a}&script=Latin&style=Hand-painted"),
    )
    .await;
    assert_eq!(summary["unmapped_count"], 3);
    let empty = get(
        &app,
        &format!("/api/v1/geo/density?{query}&unused=ok")
            .replace("style=Hand-painted", "style=Serif"),
    )
    .await;
    assert_eq!(total(&empty), 0);
    let hidden = city(&app, "XZ").await;
    sqlx::query("INSERT INTO region_policies(country_code,discoverability_enabled) VALUES('XZ',false) ON CONFLICT(country_code) DO UPDATE SET discoverability_enabled=false").execute(&app.db).await.unwrap();
    photo(
        &app,
        hidden,
        Some((77.61, 12.95)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    let density = get(
        &app,
        &format!("/api/v1/geo/density?west=77&south=12&east=78&north=13&zoom=12&city_id={hidden}"),
    )
    .await;
    assert_eq!(total(&density), 0);
    let photos = get(
        &app,
        &format!("/api/v1/geo/photos?west=77&south=12&east=78&north=13&city_id={hidden}"),
    )
    .await;
    assert_eq!(photos["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn heatmap_bounds_large_datasets_and_wraps_the_date_line() {
    let app = spawn_app().await;
    let city = city(&app, "IN").await;
    // Repeated positions exercise aggregation without jitter.
    sqlx::query("INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,location,pin_code,status,location_source) SELECT md5($1::text||g::text)::uuid,$1,'Map fixture','image','small','medium','large',ST_SetSRID(ST_MakePoint(77.60+(g%50)*0.001,12.94+(g%30)*0.001),4326)::geography,'560025','APPROVED','postal_code' FROM generate_series(1,10000) g").bind(city).execute(&app.db).await.unwrap();
    let density = get(
        &app,
        &format!(
            "/api/v1/geo/density?west=77.5&south=12.8&east=77.8&north=13.2&zoom=18&city_id={city}"
        ),
    )
    .await;
    assert_eq!(total(&density), 10000);
    assert!(density["cells"].as_array().unwrap().len() <= 1024);
    assert_eq!(density["truncated"], false);
    let photos = get(
        &app,
        &format!("/api/v1/geo/photos?west=77.5&south=12.8&east=77.8&north=13.2&city_id={city}"),
    )
    .await;
    assert_eq!(photos["items"].as_array().unwrap().len(), 30);
    let after = photos["next_cursor"].as_str().unwrap();
    let next=get(&app,&format!("/api/v1/geo/photos?west=77.5&south=12.8&east=77.8&north=13.2&city_id={city}&after={after}")).await;
    assert_ne!(photos["items"][0]["id"], next["items"][0]["id"]);
    photo(
        &app,
        city,
        Some((179.5, 0.0)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    photo(
        &app,
        city,
        Some((-179.5, 0.0)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    let wrap = get(
        &app,
        &format!("/api/v1/geo/density?west=179&south=-1&east=-179&north=1&zoom=8&city_id={city}"),
    )
    .await;
    assert_eq!(total(&wrap), 2);
    for cell in wrap["cells"].as_array().unwrap() {
        assert!(cell["lng"].as_f64().unwrap().abs() > 179.0);
    }
    let wrap = get(
        &app,
        &format!("/api/v1/geo/photos?west=179&south=-1&east=-179&north=1&city_id={city}"),
    )
    .await;
    assert_eq!(wrap["items"].as_array().unwrap().len(), 2);
    photo(
        &app,
        city,
        Some((-180.0, -85.0511287798066)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    photo(
        &app,
        city,
        Some((180.0, 85.0511287798066)),
        "postal_code",
        "APPROVED",
        "Latin",
    )
    .await;
    let world=get(&app,&format!("/api/v1/geo/density?west=-180&south=-85.0511287798066&east=180&north=85.0511287798066&zoom=0&city_id={city}")).await;
    assert_eq!(total(&world), 10004);
    for cell in world["cells"].as_array().unwrap() {
        let b = &cell["bounds"];
        assert!(b["west"].as_f64().unwrap() < b["east"].as_f64().unwrap());
        assert!(b["south"].as_f64().unwrap() < b["north"].as_f64().unwrap());
        if cell["lat"].as_f64().unwrap().abs() > 80.0 {
            let photos = get(
                &app,
                &format!(
                    "/api/v1/geo/photos?west={}&south={}&east={}&north={}&city_id={city}",
                    b["west"], b["south"], b["east"], b["north"]
                ),
            )
            .await;
            assert_eq!(photos["items"].as_array().unwrap().len(), 1);
        }
    }
    for bounds in [
        "west=NaN&south=0&east=1&north=1&zoom=12",
        "west=0&south=1&east=1&north=0&zoom=12",
        "west=0&south=0&east=0&north=1&zoom=12",
        "west=0&south=0&east=1&north=1&zoom=23",
    ] {
        let response = send(
            &app.app,
            Request::builder()
                .uri(format!("/api/v1/geo/density?{bounds}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    sqlx::query("DELETE FROM letterings WHERE city_id=$1")
        .bind(city)
        .execute(&app.db)
        .await
        .unwrap();
}
