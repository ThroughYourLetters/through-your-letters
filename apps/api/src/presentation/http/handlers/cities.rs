use axum::{
    Json,
    extract::{Path, Query, State},
};
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use std::time::Duration;
use uuid::Uuid;

use crate::presentation::http::{errors::AppError, state::AppState};

#[derive(Debug, Serialize, FromRow)]
pub struct City {
    pub id: Uuid,
    pub name: String,
    pub country_code: String,
    pub center_lat: Option<f64>,
    pub center_lng: Option<f64>,
    pub default_zoom: Option<i32>,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CityListQuery {
    pub q: Option<String>,
    pub country_code: Option<String>,
    #[serde(default = "default_city_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub discover: bool,
}

#[derive(Debug, Serialize, Default, Clone, Copy)]
pub struct CitySyncResult {
    pub processed: usize,
    pub upserted: usize,
    pub failed: usize,
}

fn default_city_limit() -> i64 {
    100
}

fn city_discovery_user_agent(state: &AppState) -> String {
    state
        .config
        .city_discovery_user_agent
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("through-your-letters/1.0 ({})", state.config.admin_email))
}

pub async fn list_cities(
    State(state): State<AppState>,
    Query(params): Query<CityListQuery>,
) -> Result<Json<Vec<City>>, AppError> {
    let q = params.q.as_deref().map(str::trim).filter(|s| !s.is_empty());

    if params.discover {
        if let Some(query) = q {
            if query.len() >= 2 {
                let _ = discover_and_cache_cities(
                    &state,
                    query,
                    params.country_code.as_deref(),
                    params.limit.clamp(1, 50),
                )
                .await;
            }
        }
    }

    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT id, name, country_code, center_lat, center_lng, default_zoom, description, cover_image_url, is_active FROM cities",
    );

    let mut has_where = false;

    if let Some(query) = q {
        qb.push(" WHERE name ILIKE ");
        qb.push_bind(format!("%{}%", query));
        has_where = true;
    }

    if let Some(country_code) = params
        .country_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if has_where {
            qb.push(" AND ");
        } else {
            qb.push(" WHERE ");
        }
        qb.push("country_code = ");
        qb.push_bind(country_code.to_uppercase());
    }

    qb.push(" ORDER BY is_active DESC, name ASC LIMIT ");
    qb.push_bind(params.limit.clamp(1, 500));
    qb.push(" OFFSET ");
    qb.push_bind(params.offset.max(0));

    let cities: Vec<City> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::InternalError(e.to_string()))?;

    Ok(Json(cities))
}

pub async fn get_city(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let city: City = sqlx::query_as(
        "SELECT id, name, country_code, center_lat, center_lng, default_zoom, description, cover_image_url, is_active FROM cities WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::InternalError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("City not found".into()))?;

    let count: (Option<i64>,) = sqlx::query_as(
        "SELECT COUNT(*) FROM letterings WHERE city_id = $1 AND status = 'APPROVED'",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::InternalError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "id": city.id,
        "name": city.name,
        "country_code": city.country_code,
        "center_lat": city.center_lat,
        "center_lng": city.center_lng,
        "default_zoom": city.default_zoom,
        "description": city.description,
        "cover_image_url": city.cover_image_url,
        "is_active": city.is_active,
        "lettering_count": count.0.unwrap_or(0),
    })))
}

#[derive(Debug, Serialize, FromRow)]
pub struct CityNeighborhoodStat {
    pub pin_code: String,
    pub count: i64,
}

pub async fn get_city_stats(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CityNeighborhoodStat>>, AppError> {
    let stats: Vec<CityNeighborhoodStat> = sqlx::query_as(
        r#"SELECT pin_code, COUNT(*)::bigint AS count
           FROM letterings
           WHERE city_id = $1 AND status = 'APPROVED'
           GROUP BY pin_code
           ORDER BY count DESC"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::InternalError(e.to_string()))?;

    Ok(Json(stats))
}

#[derive(Debug, Deserialize)]
struct NominatimAddress {
    country_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NominatimPlace {
    name: Option<String>,
    display_name: String,
    lat: String,
    lon: String,
    address: Option<NominatimAddress>,
    addresstype: Option<String>,
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WikiThumbnail {
    source: String,
}

#[derive(Debug, Deserialize)]
struct WikiSummary {
    extract: Option<String>,
    thumbnail: Option<WikiThumbnail>,
}

#[derive(Debug, Deserialize)]
struct RestCountryName {
    common: String,
}

#[derive(Debug, Deserialize)]
struct RestCapitalInfo {
    latlng: Option<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct RestCountry {
    cca2: Option<String>,
    name: Option<RestCountryName>,
    capital: Option<Vec<String>>,
    #[serde(rename = "capitalInfo")]
    capital_info: Option<RestCapitalInfo>,
}

#[derive(Debug)]
struct WikiSummaryData {
    extract: Option<String>,
    image_url: Option<String>,
}

fn is_city_like(place: &NominatimPlace) -> bool {
    let city_types = [
        "city",
        "town",
        "village",
        "municipality",
        "suburb",
        "county",
    ];

    place
        .addresstype
        .as_deref()
        .map(|v| city_types.contains(&v))
        .unwrap_or(false)
        || place
            .r#type
            .as_deref()
            .map(|v| city_types.contains(&v))
            .unwrap_or(false)
}

fn city_name_from_place(place: &NominatimPlace) -> String {
    place
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            place
                .display_name
                .split(',')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown")
                .to_string()
        })
}

async fn fetch_wikipedia_summary(
    client: &reqwest::Client,
    user_agent: &str,
    city_name: &str,
) -> Option<WikiSummaryData> {
    let title = city_name.replace(' ', "_");
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        title
    );

    let summary = client
        .get(url)
        .header(USER_AGENT, user_agent)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<WikiSummary>()
        .await
        .ok()?;

    Some(WikiSummaryData {
        extract: summary.extract,
        image_url: summary.thumbnail.map(|t| t.source),
    })
}

async fn upsert_city(
    state: &AppState,
    name: &str,
    country_code: &str,
    lat: f64,
    lng: f64,
    default_zoom: i32,
    description: Option<String>,
    cover_image_url: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO cities (id, name, country_code, center_lat, center_lng, default_zoom, description, cover_image_url, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true)
        ON CONFLICT (name, country_code) DO UPDATE
        SET center_lat = EXCLUDED.center_lat,
            center_lng = EXCLUDED.center_lng,
            default_zoom = EXCLUDED.default_zoom,
            description = COALESCE(cities.description, EXCLUDED.description),
            cover_image_url = COALESCE(cities.cover_image_url, EXCLUDED.cover_image_url),
            is_active = true
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(name)
    .bind(country_code)
    .bind(lat)
    .bind(lng)
    .bind(default_zoom)
    .bind(description)
    .bind(cover_image_url)
    .execute(&state.db)
    .await?;

    Ok(())
}

pub async fn discover_and_cache_cities(
    state: &AppState,
    query: &str,
    country_code: Option<&str>,
    limit: i64,
) -> CitySyncResult {
    let user_agent = city_discovery_user_agent(state);
    let mut result = CitySyncResult::default();

    let client = match reqwest::Client::builder().timeout(Duration::from_secs(12)).build() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!("city discovery client init failed: {}", err);
            result.failed += 1;
            return result;
        }
    };

    let mut url = match reqwest::Url::parse("https://nominatim.openstreetmap.org/search") {
        Ok(u) => u,
        Err(err) => {
            tracing::warn!("city discovery url parse failed: {}", err);
            result.failed += 1;
            return result;
        }
    };

    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", query)
            .append_pair("format", "jsonv2")
            .append_pair("addressdetails", "1")
            .append_pair("limit", &limit.clamp(1, 50).to_string());

        if let Some(code) = country_code.map(str::trim).filter(|v| !v.is_empty()) {
            qp.append_pair("countrycodes", &code.to_lowercase());
        }
    }

    let places = match client.get(url).header(USER_AGENT, &user_agent).send().await {
        Ok(res) => match res.error_for_status() {
            Ok(ok) => match ok.json::<Vec<NominatimPlace>>().await {
                Ok(data) => data,
                Err(err) => {
                    tracing::warn!("city discovery decode failed: {}", err);
                    result.failed += 1;
                    return result;
                }
            },
            Err(err) => {
                tracing::warn!("city discovery request failed: {}", err);
                result.failed += 1;
                return result;
            }
        },
        Err(err) => {
            tracing::warn!("city discovery network failed: {}", err);
            result.failed += 1;
            return result;
        }
    };

    for place in places.into_iter().filter(is_city_like).take(limit.clamp(1, 50) as usize) {
        let name = city_name_from_place(&place);
        if name.eq_ignore_ascii_case("unknown") {
            continue;
        }

        let lat = match place.lat.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                result.failed += 1;
                continue;
            }
        };
        let lng = match place.lon.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                result.failed += 1;
                continue;
            }
        };

        let detected_country = place
            .address
            .as_ref()
            .and_then(|a| a.country_code.clone())
            .map(|v| v.to_uppercase())
            .unwrap_or_else(|| "XX".to_string());

        let wiki = fetch_wikipedia_summary(&client, &user_agent, &name).await;

        let description = wiki
            .as_ref()
            .and_then(|w| w.extract.clone())
            .filter(|v| !v.trim().is_empty());

        let cover_image_url = wiki
            .as_ref()
            .and_then(|w| w.image_url.clone())
            .filter(|v| !v.trim().is_empty());

        result.processed += 1;
        match upsert_city(
            state,
            &name,
            &detected_country,
            lat,
            lng,
            12,
            description,
            cover_image_url,
        )
        .await
        {
            Ok(()) => result.upserted += 1,
            Err(err) => {
                tracing::warn!("city discovery upsert failed for {}: {}", name, err);
                result.failed += 1;
            }
        }
    }

    result
}

pub async fn bootstrap_capitals_from_restcountries(
    state: &AppState,
    limit: i64,
) -> Result<CitySyncResult, AppError> {
    let user_agent = city_discovery_user_agent(state);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    let countries = client
        .get("https://restcountries.com/v3.1/all?fields=cca2,name,capital,capitalInfo")
        .header(USER_AGENT, &user_agent)
        .send()
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::InternalError(e.to_string()))?
        .json::<Vec<RestCountry>>()
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    let mut result = CitySyncResult::default();
    let max = limit.clamp(1, 500) as usize;

    for country in countries {
        if result.processed >= max {
            break;
        }

        let country_code = match country.cca2.as_deref().map(str::trim) {
            Some(v) if !v.is_empty() => v.to_uppercase(),
            _ => continue,
        };

        let country_name = country
            .name
            .as_ref()
            .map(|n| n.common.clone())
            .unwrap_or_else(|| country_code.clone());

        let capital_name = match country.capital.as_ref().and_then(|caps| caps.first()) {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => continue,
        };

        let latlng = match country
            .capital_info
            .as_ref()
            .and_then(|c| c.latlng.as_ref())
            .filter(|coords| coords.len() >= 2)
        {
            Some(coords) => (coords[0], coords[1]),
            None => continue,
        };

        let wiki = fetch_wikipedia_summary(&client, &user_agent, &capital_name).await;
        let description = wiki
            .as_ref()
            .and_then(|w| w.extract.clone())
            .filter(|v| !v.trim().is_empty())
            .or_else(|| Some(format!("Capital city of {}", country_name)));

        let cover_image_url = wiki
            .as_ref()
            .and_then(|w| w.image_url.clone())
            .filter(|v| !v.trim().is_empty());

        result.processed += 1;
        match upsert_city(
            state,
            &capital_name,
            &country_code,
            latlng.1,
            latlng.0,
            11,
            description,
            cover_image_url,
        )
        .await
        {
            Ok(()) => result.upserted += 1,
            Err(err) => {
                tracing::warn!(
                    "capital bootstrap upsert failed for {} ({}): {}",
                    capital_name,
                    country_code,
                    err
                );
                result.failed += 1;
            }
        }
    }

    Ok(result)
}
