use std::time::Duration;

/// Geocode a postal/zip code to (longitude, latitude) using the Nominatim
/// OpenStreetMap API.  Works universally for any country — no API key needed.
///
/// - `postal_code`: the raw postal/zip string (e.g. "560001", "SW1A 1AA", "10001")
/// - `country_code`: ISO 3166-1 alpha-2 code from the `cities` table (e.g. "IN", "US", "GB")
/// - `user_agent`: required by Nominatim's usage policy; use the app's configured value
/// - `fallback`: `(lng, lat)` to use when the API is unreachable or returns no results
///
/// Always returns a coordinate — never fails the upload.
pub async fn geocode_postal_code(
    postal_code: &str,
    country_code: &str,
    user_agent: Option<&str>,
    fallback: (f64, f64),
) -> (f64, f64) {
    let agent = user_agent.unwrap_or("through-your-letters/1.0 (contact@throughyourletters.online)");

    // Nominatim requires lowercase country codes in the `countrycodes` param.
    let cc = country_code.to_lowercase();
    let url = format!(
        "https://nominatim.openstreetmap.org/search\
         ?postalcode={}&countrycodes={}&format=json&limit=1&addressdetails=0",
        postal_code, cc
    );

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(postal_code, "Could not build reqwest client for Nominatim: {}", e);
            return fallback;
        }
    };

    let response = match client
        .get(&url)
        .header(reqwest::header::USER_AGENT, agent)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                postal_code,
                country_code,
                "Nominatim request failed: {}. Using city-center fallback.",
                e
            );
            return fallback;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(
            postal_code,
            country_code,
            status = %response.status(),
            "Nominatim returned non-success status. Using city-center fallback."
        );
        return fallback;
    }

    match response.json::<serde_json::Value>().await {
        Ok(json) => {
            let coords = json
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|obj| {
                    let lat = obj["lat"].as_str()?.parse::<f64>().ok()?;
                    let lon = obj["lon"].as_str()?.parse::<f64>().ok()?;
                    Some((lon, lat))
                });

            match coords {
                Some((lon, lat)) => {
                    tracing::debug!(
                        postal_code,
                        country_code,
                        lat,
                        lon,
                        "Nominatim geocoded postal code successfully"
                    );
                    (lon, lat)
                }
                None => {
                    tracing::debug!(
                        postal_code,
                        country_code,
                        "Nominatim returned no results. Using city-center fallback."
                    );
                    fallback
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                postal_code,
                "Nominatim response parse error: {}. Using city-center fallback.",
                e
            );
            fallback
        }
    }
}
