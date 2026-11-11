use crate::{limits::PaginationLimits, presentation::http::errors::AppError};
use axum::http::{HeaderMap, HeaderValue, header};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimePosition {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursorQuery {
    pub limit: Option<i64>,
    pub after: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor<T> {
    version: u8,
    scope: String,
    position: T,
}

pub fn encode_cursor<T: Serialize>(scope: &str, position: T) -> Result<String, AppError> {
    serde_json::to_string(&Cursor {
        version: 1,
        scope: scope.to_owned(),
        position,
    })
    .map_err(AppError::internal)
}

/// Remove the look-ahead row and retain the last returned sort position.
pub fn continuation<T, P: Serialize>(
    items: &mut Vec<T>,
    page: Page,
    scope: &str,
    position: impl FnOnce(&T) -> P,
) -> Result<Option<String>, AppError> {
    if items.len() <= page.limit() as usize {
        return Ok(None);
    }
    items.truncate(page.limit() as usize);
    items
        .last()
        .map(|item| encode_cursor(scope, position(item)))
        .transpose()
}

pub fn decode_cursor<T: DeserializeOwned>(
    raw: Option<&str>,
    scope: &str,
    limits: &PaginationLimits,
) -> Result<Option<T>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() > limits.max_cursor_bytes {
        return Err(AppError::BadRequest("Cursor is too large".into()));
    }
    let cursor: Cursor<T> =
        serde_json::from_str(raw).map_err(|_| AppError::BadRequest("Invalid cursor".into()))?;
    if cursor.version != 1 || cursor.scope != scope {
        return Err(AppError::BadRequest(
            "Cursor does not match this list or its filters".into(),
        ));
    }
    Ok(Some(cursor.position))
}

/// Array responses keep their existing body and expose continuation through Link.
pub fn next_link(
    path: &str,
    query: &[(&str, String)],
    cursor: Option<&str>,
) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    if let Some(cursor) = cursor {
        let mut url =
            reqwest::Url::parse(&format!("http://localhost{path}")).map_err(AppError::internal)?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())))
            .append_pair("after", cursor);
        let link = format!(
            "<{}?{}>; rel=\"next\"",
            url.path(),
            url.query().unwrap_or_default()
        );
        headers.insert(
            header::LINK,
            HeaderValue::from_str(&link).map_err(AppError::internal)?,
        );
    }
    Ok(headers)
}

/// Validated bounds for a single database page.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    limit: i64,
    offset: i64,
}

impl Page {
    pub fn new(
        limit: i64,
        offset: i64,
        max_limit: i64,
        policy: &PaginationLimits,
    ) -> Result<Self, AppError> {
        if !(1..=max_limit).contains(&limit) {
            return Err(AppError::BadRequest(format!(
                "limit must be between 1 and {max_limit}"
            )));
        }
        if !(0..=policy.max_offset).contains(&offset) {
            return Err(AppError::BadRequest(format!(
                "offset must be between 0 and {}",
                policy.max_offset
            )));
        }
        Ok(Self { limit, offset })
    }

    pub fn limit(self) -> i64 {
        self.limit
    }
    pub fn offset(self) -> i64 {
        self.offset
    }
}
