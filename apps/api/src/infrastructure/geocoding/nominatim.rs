use crate::domain::lettering::entity::Coordinates;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum GeocodeError {
    #[error("provider_unavailable")]
    Unavailable,
    #[error("no_results")]
    NoResults,
    #[error("invalid_response")]
    InvalidResponse,
}

#[derive(Deserialize)]
struct PostalLocation {
    lon: String,
    lat: String,
}

fn coordinates(results: &[PostalLocation]) -> Result<Coordinates, GeocodeError> {
    let first = results.first().ok_or(GeocodeError::NoResults)?;
    let point = Coordinates::new_point(
        first
            .lon
            .parse()
            .map_err(|_| GeocodeError::InvalidResponse)?,
        first
            .lat
            .parse()
            .map_err(|_| GeocodeError::InvalidResponse)?,
    );
    if !point.is_valid() {
        return Err(GeocodeError::InvalidResponse);
    }
    Ok(point)
}

/// Postal centroids are approximate, never GPS positions.
pub async fn geocode_postal_code(
    postal_code: &str,
    country_code: &str,
    user_agent: Option<&str>,
) -> Result<Coordinates, GeocodeError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|error| {
            tracing::warn!(error = %error.without_url(), "Postal geocoding request failed");
            GeocodeError::Unavailable
        })?;
    lookup(&client, postal_url(postal_code, country_code), user_agent).await
}

fn postal_url(postal_code: &str, country_code: &str) -> reqwest::Url {
    let mut url = reqwest::Url::parse("https://nominatim.openstreetmap.org/search")
        .expect("static geocoding URL is valid");
    url.query_pairs_mut().extend_pairs([
        ("postalcode", postal_code),
        ("countrycodes", &country_code.to_ascii_lowercase()),
        ("format", "json"),
        ("limit", "1"),
        ("addressdetails", "0"),
    ]);
    url
}

async fn lookup(
    client: &reqwest::Client,
    url: reqwest::Url,
    user_agent: Option<&str>,
) -> Result<Coordinates, GeocodeError> {
    let results = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            user_agent.unwrap_or("through-your-letters/1.0 (contact@throughyourletters.online)"),
        )
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| {
            tracing::warn!(error = %error.without_url(), "Postal geocoding request failed");
            GeocodeError::Unavailable
        })?
        .json::<Vec<PostalLocation>>()
        .await
        .map_err(|error| {
            tracing::warn!(error = %error.without_url(), "Postal geocoding response was invalid");
            GeocodeError::InvalidResponse
        })?;
    coordinates(&results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_results_stay_unresolved() {
        assert!(matches!(coordinates(&[]), Err(GeocodeError::NoResults)));
    }

    #[test]
    fn rejects_invalid_provider_coordinates() {
        for (lon, lat) in [
            ("NaN", "12"),
            ("181", "12"),
            ("77", "91"),
            ("77", "inf"),
            ("bad", "12"),
        ] {
            let result = coordinates(&[PostalLocation {
                lon: lon.into(),
                lat: lat.into(),
            }]);
            assert!(matches!(result, Err(GeocodeError::InvalidResponse)));
        }
    }

    #[test]
    fn preserves_longitude_latitude_order() {
        let point = coordinates(&[PostalLocation {
            lon: "77.61".into(),
            lat: "12.95".into(),
        }])
        .unwrap();
        assert_eq!(point.coordinates, vec![77.61, 12.95]);
    }
    #[test]
    fn postal_parameters_cannot_change_the_query() {
        let url = postal_url("SW1A 1AA&limit=20", "GB");
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params["postalcode"], "SW1A 1AA&limit=20");
        assert_eq!(params["countrycodes"], "gb");
        assert_eq!(params["limit"], "1");
    }

    #[tokio::test]
    async fn provider_failures_do_not_create_coordinates() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (status, body, expected) in [
            ("200 OK", "[]", "no_results"),
            ("200 OK", "not json", "invalid_response"),
            (
                "200 OK",
                r#"[{"lon":"999","lat":"12"}]"#,
                "invalid_response",
            ),
            ("429 Too Many Requests", "[]", "provider_unavailable"),
            ("503 Service Unavailable", "[]", "provider_unavailable"),
            ("200 OK", r#"[{"lon":"77.61","lat":"12.95"}]"#, "ok"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let mut read = 0;
                while !request[..read].ends_with(b"\r\n\r\n") {
                    let count = stream.read(&mut request[read..]).await.unwrap();
                    assert!(count > 0, "incomplete HTTP request");
                    read += count;
                }
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap();
            let result = lookup(
                &client,
                format!("http://{address}/").parse().unwrap(),
                Some("test"),
            )
            .await;
            match result {
                Ok(point) => {
                    assert_eq!(expected, "ok");
                    assert!(point.is_valid());
                }
                Err(error) => assert_eq!(error.to_string(), expected),
            }
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn connection_errors_and_timeouts_stay_unresolved() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url: reqwest::Url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        assert!(matches!(
            lookup(&client, url.clone(), None).await,
            Err(GeocodeError::Unavailable)
        ));
        drop(listener);
        assert!(matches!(
            lookup(&client, url, None).await,
            Err(GeocodeError::Unavailable)
        ));
    }
}
