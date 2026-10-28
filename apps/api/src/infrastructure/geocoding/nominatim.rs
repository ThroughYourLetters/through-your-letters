use crate::{domain::lettering::entity::Coordinates, limits::GeocodingLimits};
use reqwest::{Client, Url, header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GeocodeError {
    #[error("provider_unavailable")]
    Unavailable,
    #[error("provider_rate_limited")]
    RateLimited,
    #[error("no_results")]
    NoResults,
    #[error("invalid_response")]
    InvalidResponse,
    #[error("invalid_query")]
    InvalidQuery,
    #[error("cache_unavailable")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaceAddress {
    pub country_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Place {
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: String,
    pub lat: String,
    pub lon: String,
    pub address: Option<PlaceAddress>,
    pub addresstype: Option<String>,
    pub r#type: Option<String>,
}

impl Place {
    fn coordinates(&self) -> Result<Coordinates, GeocodeError> {
        let point = Coordinates::new_point(
            self.lon
                .parse()
                .map_err(|_| GeocodeError::InvalidResponse)?,
            self.lat
                .parse()
                .map_err(|_| GeocodeError::InvalidResponse)?,
        );
        if !point.is_valid() {
            return Err(GeocodeError::InvalidResponse);
        }
        Ok(point)
    }
}

/// Shares cached searches and provider limits through PostgreSQL.
/// Cache or gate failures prevent external requests. Uses no Redis commands.
#[derive(Clone)]
pub struct Geocoder {
    db: PgPool,
    client: Client,
    endpoint: Url,
    limits: GeocodingLimits,
}

impl Geocoder {
    pub fn new(db: PgPool, limits: GeocodingLimits, user_agent: &str) -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_millis(limits.request_timeout_ms.into()))
            // Redirects and retries bypass provider budgeting.
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()?;
        let endpoint = Url::parse(&limits.search_url)?;
        Ok(Self {
            db,
            client,
            endpoint,
            limits,
        })
    }

    /// Returns approximate postal coordinates, never GPS positions.
    /// Busy providers leave uploads unresolved; callers may retain the upload.
    pub async fn postal_code(
        &self,
        postal: &str,
        country: &str,
    ) -> Result<Coordinates, GeocodeError> {
        let postal: String = postal.chars().filter(|c| !c.is_whitespace()).collect();
        if postal.is_empty()
            || postal.len() > 32
            || !postal
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(GeocodeError::InvalidQuery);
        }
        let country = normalize_country(country)?;
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().extend_pairs([
            ("postalcode", postal.to_ascii_uppercase().as_str()),
            ("countrycodes", &country),
            ("format", "jsonv2"),
            ("limit", "1"),
            ("addressdetails", "0"),
        ]);
        let places = self.search(url).await?;
        places.first().ok_or(GeocodeError::NoResults)?.coordinates()
    }

    pub async fn cities(
        &self,
        query: &str,
        country: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Place>, GeocodeError> {
        let query = query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if query.is_empty() || query.len() > 200 {
            return Err(GeocodeError::InvalidQuery);
        }
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().extend_pairs([
            ("q", query.as_str()),
            ("format", "jsonv2"),
            ("addressdetails", "1"),
            ("limit", limit.clamp(1, 50).to_string().as_str()),
        ]);
        if let Some(country) = country.filter(|c| !c.trim().is_empty()) {
            url.query_pairs_mut()
                .append_pair("countrycodes", &normalize_country(country)?);
        }
        self.search(url).await
    }

    async fn search(&self, url: Url) -> Result<Vec<Place>, GeocodeError> {
        tokio::time::timeout(
            Duration::from_millis(self.limits.total_timeout_ms.into()),
            self.search_inner(url),
        )
        .await
        .map_err(|_| GeocodeError::Unavailable)?
    }

    async fn cached(&self, key: &str) -> Result<Option<Vec<Place>>, GeocodeError> {
        let row: Option<Json<Vec<Place>>> = sqlx::query_scalar(
            "SELECT response FROM geocoding_cache WHERE cache_key = $1 AND expires_at > clock_timestamp()"
        ).bind(key).fetch_optional(&self.db).await?;
        Ok(row.map(|Json(places)| places))
    }

    async fn search_inner(&self, url: Url) -> Result<Vec<Place>, GeocodeError> {
        let key = format!("{:x}", Sha256::digest(url.as_str().as_bytes()));
        if let Some(places) = self.cached(&key).await? {
            return Ok(places);
        }
        let lease = Uuid::now_v7();
        // Expired leases recover after cancelled requests.
        let hold_ms = self.limits.total_timeout_ms + self.limits.min_interval_ms;
        let claimed: Option<bool> = sqlx::query_scalar(
            "UPDATE geocoding_gate SET lease_id = $1, next_allowed_at = clock_timestamp() + $2 * interval '1 millisecond' \
             WHERE id AND next_allowed_at <= clock_timestamp() RETURNING id"
        ).bind(lease).bind(f64::from(hold_ms)).fetch_optional(&self.db).await?;
        if claimed.is_none() {
            return Err(GeocodeError::RateLimited);
        }
        // Another replica may have filled cache.
        if let Some(places) = self.cached(&key).await? {
            self.release(lease, 0).await?;
            return Ok(places);
        }
        match self.fetch(url).await {
            Ok(places) => {
                self.store(lease, &key, &places).await?;
                Ok(places)
            }
            Err((error, cooldown_ms)) => {
                self.release(lease, cooldown_ms).await?;
                Err(error)
            }
        }
    }

    async fn release(&self, lease: Uuid, delay_ms: u64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE geocoding_gate SET lease_id = NULL, next_allowed_at = clock_timestamp() + $2 * interval '1 millisecond' \
             WHERE id AND lease_id = $1"
        ).bind(lease).bind(delay_ms as f64).execute(&self.db).await?;
        Ok(())
    }

    async fn store(&self, lease: Uuid, key: &str, places: &[Place]) -> Result<(), GeocodeError> {
        let mut tx = self.db.begin().await?;
        // Keep eviction and lease release atomic.
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT lease_id FROM geocoding_gate WHERE id FOR UPDATE")
                .fetch_one(&mut *tx)
                .await?;
        if owner != Some(lease) {
            return Err(GeocodeError::RateLimited);
        }
        let ttl = if places.is_empty() {
            self.limits.negative_ttl_seconds
        } else {
            self.limits.positive_ttl_seconds
        };
        sqlx::query("DELETE FROM geocoding_cache WHERE expires_at <= clock_timestamp()")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO geocoding_cache (cache_key, response, expires_at) \
             VALUES ($1, $2, clock_timestamp() + $3 * interval '1 second') \
             ON CONFLICT (cache_key) DO UPDATE SET response = EXCLUDED.response, expires_at = EXCLUDED.expires_at, updated_at = clock_timestamp()"
        ).bind(key).bind(Json(places)).bind(f64::from(ttl)).execute(&mut *tx).await?;
        sqlx::query(
            "DELETE FROM geocoding_cache WHERE cache_key IN \
             (SELECT cache_key FROM geocoding_cache ORDER BY updated_at, cache_key \
              LIMIT GREATEST((SELECT count(*) FROM geocoding_cache) - $1, 0))",
        )
        .bind(i64::from(self.limits.max_entries))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE geocoding_gate SET lease_id = NULL, next_allowed_at = clock_timestamp() + $2 * interval '1 millisecond' WHERE id AND lease_id = $1"
        ).bind(lease).bind(f64::from(self.limits.min_interval_ms)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn fetch(&self, url: Url) -> Result<Vec<Place>, (GeocodeError, u64)> {
        let fallback = (u64::from(self.limits.failure_cooldown_seconds) * 1000)
            .max(u64::from(self.limits.min_interval_ms));
        let mut response = self.client.get(url).send().await.map_err(|error| {
            tracing::warn!(error = %error.without_url(), "Geocoding provider request failed");
            (GeocodeError::Unavailable, fallback)
        })?;
        let cooldown = retry_after_ms(response.headers())
            .unwrap_or(fallback)
            .max(fallback);
        if !response.status().is_success() {
            return Err((
                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    GeocodeError::RateLimited
                } else {
                    GeocodeError::Unavailable
                },
                cooldown,
            ));
        }
        let max = self.limits.max_response_bytes as usize;
        if response
            .content_length()
            .is_some_and(|size| size > max as u64)
        {
            return Err((GeocodeError::InvalidResponse, fallback));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| (GeocodeError::Unavailable, fallback))?
        {
            if chunk.len() > max - body.len() {
                return Err((GeocodeError::InvalidResponse, fallback));
            }
            body.extend_from_slice(&chunk);
        }
        let places: Vec<Place> =
            serde_json::from_slice(&body).map_err(|_| (GeocodeError::InvalidResponse, fallback))?;
        if places.len() > 50
            || serde_json::to_vec(&places)
                .map_err(|_| (GeocodeError::InvalidResponse, fallback))?
                .len()
                > max
        {
            return Err((GeocodeError::InvalidResponse, fallback));
        }
        for place in &places {
            place.coordinates().map_err(|error| (error, fallback))?;
        }
        Ok(places)
    }
}

fn normalize_country(country: &str) -> Result<String, GeocodeError> {
    let country = country.trim();
    if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(GeocodeError::InvalidQuery);
    }
    Ok(country.to_ascii_lowercase())
}

fn retry_after_ms(headers: &header::HeaderMap) -> Option<u64> {
    let value = headers.get(header::RETRY_AFTER)?.to_str().ok()?;
    let seconds = match value.parse::<u32>() {
        Ok(seconds) => u64::from(seconds),
        Err(_) => (chrono::DateTime::parse_from_rfc2822(value)
            .ok()?
            .timestamp()
            - chrono::Utc::now().timestamp())
        .max(0) as u64,
    };
    seconds.checked_mul(1000)
}

#[cfg(test)]
mod tests;
