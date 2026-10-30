//! Reviewed postal-area corrections. Preview never changes coordinates.
use crate::presentation::http::{
    errors::AppError, middleware::admin::AdminClaims, state::AppState,
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LocationRecord {
    pub id: Uuid,
    pub city_name: String,
    pub country_code: String,
    pub pin_code: String,
    pub image_url: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub location_source: String,
    pub location_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

const RECORD_SQL: &str =
    "SELECT l.id, c.name AS city_name, c.country_code, l.pin_code, l.image_url,
    ST_Y(l.location::geometry) AS lat, ST_X(l.location::geometry) AS lng,
    l.location_source, l.location_error, l.updated_at
    FROM letterings l JOIN cities c ON c.id=l.city_id";

#[derive(Deserialize)]
pub struct ReportQuery {
    #[serde(default)]
    pub include_resolved: bool,
    pub city_id: Option<Uuid>,
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct Report {
    pub items: Vec<LocationRecord>,
    pub next_cursor: Option<Uuid>,
}

/// Keyset pagination keeps repair reports resumable.
pub async fn report(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<Report>, AppError> {
    let limit = query
        .limit
        .unwrap_or(state.config.limits.gallery.default_limit)
        .clamp(1, state.config.limits.gallery.max_limit);
    let mut sql = QueryBuilder::<Postgres>::new(RECORD_SQL);
    sql.push(" WHERE true");
    if !query.include_resolved {
        sql.push(" AND l.location_source IN ('legacy_unverified','unresolved')");
    }
    if let Some(city_id) = query.city_id {
        sql.push(" AND l.city_id = ").push_bind(city_id);
    }
    if let Some(after) = query.after {
        sql.push(" AND l.id > ").push_bind(after);
    }
    sql.push(" ORDER BY l.id LIMIT ").push_bind(limit + 1);
    let mut items: Vec<LocationRecord> = sql.build_query_as().fetch_all(&state.db).await?;
    let next_cursor = if items.len() > limit as usize {
        items.truncate(limit as usize);
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(Report { items, next_cursor }))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub lat: f64,
    pub lng: f64,
    pub city_name: String,
    pub country_code: String,
    pub pin_code: String,
    pub source_name: String,
    pub source_url: String,
    pub reason: String,
}

fn postal(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

impl Evidence {
    fn validate(&self, current: &LocationRecord, max_bytes: usize) -> Result<(), AppError> {
        let size: usize = [
            &self.city_name,
            &self.country_code,
            &self.pin_code,
            &self.source_name,
            &self.source_url,
            &self.reason,
        ]
        .iter()
        .map(|value| value.len())
        .sum();
        if size > max_bytes {
            return Err(AppError::BadRequest("Source evidence is too long".into()));
        }
        if self.city_name.trim().is_empty()
            || self.country_code.trim().is_empty()
            || postal(&self.pin_code).is_empty()
        {
            return Err(AppError::BadRequest(
                "Include the source city, country, and postal code".into(),
            ));
        }
        let point = crate::domain::lettering::entity::Coordinates::new_point(self.lng, self.lat);
        if !point.is_valid() {
            return Err(AppError::BadRequest(
                "Enter valid latitude and longitude".into(),
            ));
        }
        if self.city_name.trim().to_lowercase() != current.city_name.trim().to_lowercase()
            || !self
                .country_code
                .trim()
                .eq_ignore_ascii_case(&current.country_code)
            || postal(&self.pin_code) != postal(&current.pin_code)
        {
            return Err(AppError::BadRequest(
                "The source city, country, and postal code must match the upload".into(),
            ));
        }
        let url = reqwest::Url::parse(&self.source_url)
            .map_err(|_| AppError::BadRequest("Enter an HTTPS source URL".into()))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(AppError::BadRequest(
                "Enter an HTTPS source URL without credentials".into(),
            ));
        }
        if self.source_name.trim().is_empty() || self.reason.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Name the source and explain the correction".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Preview {
    pub repair_id: Uuid,
    pub current: LocationRecord,
    pub evidence: Evidence,
}

pub async fn preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(evidence): Json<Evidence>,
) -> Result<Json<Preview>, AppError> {
    let current = sqlx::query_as::<_, LocationRecord>(&format!("{RECORD_SQL} WHERE l.id=$1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Upload not found".into()))?;
    evidence.validate(
        &current,
        state.config.limits.location_repairs.max_evidence_bytes,
    )?;
    Ok(Json(Preview {
        repair_id: Uuid::now_v7(),
        current,
        evidence,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub repair_id: Uuid,
    pub expected_updated_at: DateTime<Utc>,
    pub evidence: Evidence,
    pub reviewed: bool,
}

#[derive(Serialize, FromRow)]
pub struct Repair {
    pub id: Uuid,
    pub lettering_id: Uuid,
    pub reviewed_by: String,
    pub before_location: serde_json::Value,
    pub after_location: serde_json::Value,
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
const HISTORY_SQL: &str = "SELECT id, lettering_id, reviewed_by, before_location, after_location, evidence, created_at FROM location_repairs";

/// Commit coordinates and their evidence together.
pub async fn apply(
    State(state): State<AppState>,
    Extension(admin): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApplyRequest>,
) -> Result<Json<Repair>, AppError> {
    if !request.reviewed {
        return Err(AppError::BadRequest(
            "Confirm that you checked the source".into(),
        ));
    }
    let mut tx = state.db.begin().await?;
    let current = sqlx::query_as::<_, LocationRecord>(&format!(
        "{RECORD_SQL} WHERE l.id=$1 FOR UPDATE OF l,c"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Upload not found".into()))?;
    let evidence = serde_json::to_value(&request.evidence).map_err(AppError::internal)?;
    // Repeated approvals must never repeat writes.
    if let Some(existing) = sqlx::query_as::<_, Repair>(&format!("{HISTORY_SQL} WHERE id=$1"))
        .bind(request.repair_id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing.lettering_id != id
            || existing.reviewed_by != admin.sub
            || existing.evidence != evidence
            || existing.before_location["updated_at"]
                != serde_json::json!(request.expected_updated_at)
        {
            return Err(AppError::Conflict(
                "This repair ID belongs to a different correction".into(),
            ));
        }
        tx.commit().await?;
        return Ok(Json(existing));
    }
    if current.updated_at != request.expected_updated_at {
        return Err(AppError::Conflict(
            "The upload changed. Preview the correction again".into(),
        ));
    }
    request.evidence.validate(
        &current,
        state.config.limits.location_repairs.max_evidence_bytes,
    )?;
    sqlx::query("UPDATE letterings SET location=ST_SetSRID(ST_MakePoint($2,$3),4326)::geography, location_source='postal_code', location_error=NULL, updated_at=NOW() WHERE id=$1")
        .bind(id).bind(request.evidence.lng).bind(request.evidence.lat).execute(&mut *tx).await?;
    let after = sqlx::query_as::<_, LocationRecord>(&format!("{RECORD_SQL} WHERE l.id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let before = serde_json::to_value(current).map_err(AppError::internal)?;
    let after = serde_json::to_value(after).map_err(AppError::internal)?;
    let repair = sqlx::query_as::<_, Repair>("INSERT INTO location_repairs(id,lettering_id,reviewed_by,before_location,after_location,evidence) VALUES($1,$2,$3,$4,$5,$6) RETURNING id,lettering_id,reviewed_by,before_location,after_location,evidence,created_at")
        .bind(request.repair_id).bind(id).bind(&admin.sub).bind(before).bind(after).bind(evidence)
        .fetch_one(&mut *tx).await.map_err(|error| {
            if error.as_database_error().is_some_and(|e| e.is_unique_violation()) {
                AppError::Conflict("This repair ID was already used".into())
            } else { AppError::from(error) }
        })?;
    sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,lettering_id,metadata) VALUES($1,$2,'location_repair',$3,$4)")
        .bind(Uuid::now_v7()).bind(admin.sub).bind(id).bind(serde_json::json!({"repair_id": repair.id})).execute(&mut *tx).await?;
    tx.commit().await?;
    state.cache.invalidate().await;
    Ok(Json(repair))
}

#[derive(Serialize)]
pub struct History {
    pub items: Vec<Repair>,
    pub next_cursor: Option<Uuid>,
}

pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<History>, AppError> {
    let limit = query
        .limit
        .unwrap_or(state.config.limits.gallery.default_limit)
        .clamp(1, state.config.limits.gallery.max_limit);
    let mut items = sqlx::query_as::<_, Repair>(&format!("{HISTORY_SQL} WHERE lettering_id=$1 AND ($2::uuid IS NULL OR (created_at,id) < (SELECT created_at,id FROM location_repairs WHERE id=$2 AND lettering_id=$1)) ORDER BY created_at DESC,id DESC LIMIT $3"))
        .bind(id).bind(query.after).bind(limit + 1).fetch_all(&state.db).await?;
    let next_cursor = if items.len() > limit as usize {
        items.truncate(limit as usize);
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(History { items, next_cursor }))
}
