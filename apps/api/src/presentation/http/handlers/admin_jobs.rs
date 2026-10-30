use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};
use ts_rs::TS;
use uuid::Uuid;

use crate::presentation::http::{
    bounded_json::BoundedJson, errors::AppError, middleware::admin::AdminClaims, state::AppState,
};

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;

#[derive(Deserialize)]
pub struct JobsQuery {
    after: Option<Uuid>,
    limit: Option<u32>,
}

impl JobsQuery {
    fn page_size(&self) -> Result<i64, AppError> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if !(1..=MAX_PAGE_SIZE).contains(&limit) {
            return Err(AppError::BadRequest(format!(
                "Page size must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }
        Ok(i64::from(limit))
    }
}

#[derive(Serialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum AnalysisJobError {
    ProcessingFailed,
    TimedOut,
    LeaseExpired,
    Interrupted,
}

#[derive(Serialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum CleanupJobError {
    StorageUnavailable,
    UnrecognizedUrl,
}

#[derive(Serialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum CleanupJobState {
    Ready,
    Failed,
}

#[derive(Serialize, FromRow, TS)]
pub struct AnalysisJobItem {
    pub id: Uuid,
    pub contributor_tag: String,
    pub thumbnail_small: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error_code: Option<AnalysisJobError>,
    pub finished_at: DateTime<Utc>,
    pub can_retry: bool,
}

#[derive(Serialize, TS)]
pub struct AnalysisJobsResponse {
    pub items: Vec<AnalysisJobItem>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Serialize, FromRow, TS)]
pub struct CleanupJobItem {
    pub id: Uuid,
    pub state: CleanupJobState,
    pub attempts: i32,
    pub last_error_code: Option<CleanupJobError>,
    pub available_at: DateTime<Utc>,
}

#[derive(Serialize, TS)]
pub struct CleanupJobsResponse {
    pub items: Vec<CleanupJobItem>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RetryAnalysisRequest {
    pub finished_at: DateTime<Utc>,
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RetryCleanupRequest {
    pub available_at: DateTime<Utc>,
    pub attempts: u32,
}

pub async fn analysis(
    State(state): State<AppState>,
    Query(query): Query<JobsQuery>,
) -> Result<Json<AnalysisJobsResponse>, AppError> {
    let size = query.page_size()?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT j.lettering_id AS id,l.contributor_tag,l.thumbnail_small,j.attempts,j.max_attempts,j.last_error_code,j.finished_at,
            l.status='PENDING' AND l.moderated_at IS NULL AS can_retry
         FROM analysis_jobs j JOIN letterings l ON l.id=j.lettering_id
         WHERE j.state='failed'",
    );
    if let Some(after) = query.after {
        sql.push(" AND j.lettering_id>").push_bind(after);
    }
    sql.push(" ORDER BY j.lettering_id LIMIT ")
        .push_bind(size + 1);
    let mut items = sql
        .build_query_as::<AnalysisJobItem>()
        .fetch_all(&state.db)
        .await?;
    let next_cursor = if items.len() > size as usize {
        items.truncate(size as usize);
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(AnalysisJobsResponse { items, next_cursor }))
}

pub async fn cleanup(
    State(state): State<AppState>,
    Query(query): Query<JobsQuery>,
) -> Result<Json<CleanupJobsResponse>, AppError> {
    let size = query.page_size()?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT id,state,attempts,last_error_code,available_at FROM storage_cleanup
         WHERE (state='failed' OR (state='ready' AND last_error_code IS NOT NULL))",
    );
    if let Some(after) = query.after {
        sql.push(" AND id>").push_bind(after);
    }
    sql.push(" ORDER BY id LIMIT ").push_bind(size + 1);
    let mut items = sql
        .build_query_as::<CleanupJobItem>()
        .fetch_all(&state.db)
        .await?;
    let next_cursor = if items.len() > size as usize {
        items.truncate(size as usize);
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(CleanupJobsResponse { items, next_cursor }))
}

pub async fn retry_analysis(
    State(state): State<AppState>,
    Extension(admin): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    BoundedJson(request): BoundedJson<RetryAnalysisRequest>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin().await?;
    // Parent before job matches deletion, moderation, and worker completion.
    let eligible: Option<bool> = sqlx::query_scalar(
        "SELECT status='PENDING' AND moderated_at IS NULL FROM letterings WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    match eligible {
        None => return Err(AppError::NotFound("Upload no longer exists".into())),
        Some(false) => {
            return Err(AppError::Conflict(
                "This upload has already been reviewed".into(),
            ));
        }
        Some(true) => {}
    }
    let previous: Option<(i32, Option<String>)> = sqlx::query_as(
        "SELECT attempts,last_error_code FROM analysis_jobs
         WHERE lettering_id=$1 AND state='failed' AND finished_at=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(request.finished_at)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((attempts, error)) = previous else {
        return Err(AppError::Conflict(
            "The job changed; refresh before retrying".into(),
        ));
    };
    sqlx::query(
        "UPDATE analysis_jobs SET state='ready',attempts=0,last_error_code=NULL,
            available_at=clock_timestamp(),finished_at=NULL,updated_at=clock_timestamp()
         WHERE lettering_id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    audit(&mut tx, &admin.sub, "RETRY_ANALYSIS_JOB", Some(id), serde_json::json!({
        "previous_attempts": attempts, "previous_error": error, "finished_at": request.finished_at,
    })).await?;
    sqlx::query("SELECT pg_notify('ttl_analysis_jobs','')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_cleanup(
    State(state): State<AppState>,
    Extension(admin): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    BoundedJson(request): BoundedJson<RetryCleanupRequest>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin().await?;
    let previous: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT state,last_error_code FROM storage_cleanup
         WHERE id=$1 AND available_at=$2 AND attempts::bigint=$3
           AND (state='failed' OR (state='ready' AND last_error_code IS NOT NULL)) FOR UPDATE",
    )
    .bind(id)
    .bind(request.available_at)
    .bind(i64::from(request.attempts))
    .fetch_optional(&mut *tx)
    .await?;
    let Some((previous_state, error)) = previous else {
        return Err(AppError::Conflict(
            "The job changed or is active; refresh before retrying".into(),
        ));
    };
    sqlx::query("UPDATE storage_cleanup SET state='ready',available_at=clock_timestamp(),last_error_code=NULL WHERE id=$1")
        .bind(id).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &admin.sub,
        "RETRY_STORAGE_CLEANUP",
        None,
        serde_json::json!({
            "cleanup_id": id, "previous_attempts": request.attempts,
            "previous_state": previous_state, "previous_error": error,
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn audit(
    connection: &mut PgConnection,
    admin: &str,
    action: &'static str,
    lettering_id: Option<Uuid>,
    metadata: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO admin_audit_logs(id,admin_sub,action,lettering_id,metadata) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::now_v7()).bind(admin).bind(action).bind(lettering_id).bind(metadata)
        .execute(connection).await?;
    Ok(())
}
