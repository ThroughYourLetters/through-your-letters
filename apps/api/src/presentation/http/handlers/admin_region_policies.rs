use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

use crate::presentation::http::{
    errors::AppError, middleware::admin::AdminClaims, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegionPoliciesQuery {
    pub country_code: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    200
}

#[derive(Debug, Serialize, FromRow)]
pub struct RegionPolicyItem {
    pub country_code: String,
    pub uploads_enabled: bool,
    pub comments_enabled: bool,
    pub discoverability_enabled: bool,
    pub auto_moderation_level: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RegionPoliciesResponse {
    pub items: Vec<RegionPolicyItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpsertRegionPolicyRequest {
    pub uploads_enabled: Option<bool>,
    pub comments_enabled: Option<bool>,
    pub discoverability_enabled: Option<bool>,
    pub auto_moderation_level: Option<String>,
}

fn normalize_country_code(code: &str) -> Result<String, AppError> {
    let normalized = code.trim().to_uppercase();
    if normalized.len() != 2 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::BadRequest(
            "country_code must be a 2-letter ISO code".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_auto_moderation_level(level: Option<&str>) -> Result<String, AppError> {
    let normalized = level.unwrap_or("standard").trim().to_lowercase();
    if !["relaxed", "standard", "strict"].contains(&normalized.as_str()) {
        return Err(AppError::BadRequest(
            "auto_moderation_level must be one of relaxed, standard, strict".to_string(),
        ));
    }
    Ok(normalized)
}

pub async fn list_region_policies(
    State(state): State<AppState>,
    Query(params): Query<RegionPoliciesQuery>,
) -> Result<Json<RegionPoliciesResponse>, AppError> {
    let country = params
        .country_code
        .as_deref()
        .map(normalize_country_code)
        .transpose()?;

    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT country_code, uploads_enabled, comments_enabled, discoverability_enabled, auto_moderation_level, created_at, updated_at
         FROM region_policies",
    );

    if let Some(country_code) = &country {
        qb.push(" WHERE country_code = ").push_bind(country_code);
    }

    qb.push(" ORDER BY country_code ASC LIMIT ")
        .push_bind(params.limit.clamp(1, 500))
        .push(" OFFSET ")
        .push_bind(params.offset.max(0));

    let items: Vec<RegionPolicyItem> = qb
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut count_qb = QueryBuilder::<Postgres>::new("SELECT COUNT(*)::bigint FROM region_policies");
    if let Some(country_code) = &country {
        count_qb.push(" WHERE country_code = ").push_bind(country_code);
    }
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(RegionPoliciesResponse {
        items,
        total,
        limit: params.limit.clamp(1, 500),
        offset: params.offset.max(0),
    }))
}

pub async fn upsert_region_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(country_code): Path<String>,
    Json(body): Json<UpsertRegionPolicyRequest>,
) -> Result<(StatusCode, Json<RegionPolicyItem>), AppError> {
    let country_code = normalize_country_code(&country_code)?;
    let auto_moderation_level =
        normalize_auto_moderation_level(body.auto_moderation_level.as_deref())?;

    sqlx::query(
        "INSERT INTO region_policies (
            country_code, uploads_enabled, comments_enabled, discoverability_enabled, auto_moderation_level
        ) VALUES (
            $1, COALESCE($2, true), COALESCE($3, true), COALESCE($4, true), $5
        )
        ON CONFLICT (country_code) DO UPDATE
        SET uploads_enabled = COALESCE($2, region_policies.uploads_enabled),
            comments_enabled = COALESCE($3, region_policies.comments_enabled),
            discoverability_enabled = COALESCE($4, region_policies.discoverability_enabled),
            auto_moderation_level = COALESCE($5, region_policies.auto_moderation_level),
            updated_at = NOW()",
    )
    .bind(&country_code)
    .bind(body.uploads_enabled)
    .bind(body.comments_enabled)
    .bind(body.discoverability_enabled)
    .bind(auto_moderation_level)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let item = sqlx::query_as::<_, RegionPolicyItem>(
        "SELECT country_code, uploads_enabled, comments_enabled, discoverability_enabled, auto_moderation_level, created_at, updated_at
         FROM region_policies
         WHERE country_code = $1",
    )
    .bind(&country_code)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let _ = sqlx::query(
        "INSERT INTO admin_audit_logs (id, admin_sub, action, metadata, created_at)
         VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(&claims.sub)
    .bind("UPSERT_REGION_POLICY")
    .bind(serde_json::json!({
        "country_code": item.country_code,
        "uploads_enabled": item.uploads_enabled,
        "comments_enabled": item.comments_enabled,
        "discoverability_enabled": item.discoverability_enabled,
        "auto_moderation_level": item.auto_moderation_level
    }))
    .execute(&state.db)
    .await;

    Ok((StatusCode::OK, Json(item)))
}
