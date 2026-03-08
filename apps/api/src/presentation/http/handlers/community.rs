use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListCollectionsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Deserialize)]
pub struct CollectionItemsQuery {
    #[serde(default = "default_items_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 { 50 }
fn default_items_limit() -> i64 { 100 }

use crate::presentation::http::{
    errors::AppError,
    middleware::user::decode_required_user_claims,
    state::AppState,
};

// --- Leaderboard ---

#[derive(Serialize)]
pub struct LeaderboardEntry {
    pub tag: String,
    pub count: i64,
    pub total_likes: i64,
}

pub async fn get_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<Vec<LeaderboardEntry>>, AppError> {
    let rows: Vec<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT contributor_tag, COUNT(*), COALESCE(SUM(likes_count::bigint), 0) FROM letterings WHERE status = 'APPROVED' GROUP BY contributor_tag ORDER BY COUNT(*) DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(tag, count, total_likes)| LeaderboardEntry {
                tag,
                count: count.unwrap_or(0),
                total_likes: total_likes.unwrap_or(0),
            })
            .collect(),
    ))
}

// --- Collections ---

#[derive(Serialize, sqlx::FromRow)]
pub struct CollectionRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_tag: String,
    pub is_public: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct CollectionResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_tag: String,
    pub is_public: bool,
    pub item_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct CollectionListResponse {
    pub items: Vec<CollectionResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_collections(
    State(state): State<AppState>,
    Query(params): Query<ListCollectionsQuery>,
) -> Result<Json<CollectionListResponse>, AppError> {
    let safe_limit = params.limit.clamp(1, 100);
    let safe_offset = params.offset.max(0);

    type CollectionRow2 = (Uuid, String, Option<String>, String, bool, chrono::DateTime<chrono::Utc>, Option<i64>);
    let rows: Vec<CollectionRow2> = sqlx::query_as(
        "SELECT c.id, c.name, c.description, c.creator_tag, c.is_public, c.created_at, COUNT(ci.lettering_id)
         FROM collections c
         LEFT JOIN collection_items ci ON ci.collection_id = c.id
         WHERE c.is_public = true
         GROUP BY c.id
         ORDER BY c.created_at DESC
         LIMIT $1 OFFSET $2",
    )
    .bind(safe_limit)
    .bind(safe_offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM collections WHERE is_public = true")
            .fetch_one(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let items = rows
        .into_iter()
        .map(
            |(id, name, description, creator_tag, is_public, created_at, item_count)| {
                CollectionResponse {
                    id,
                    name,
                    description,
                    creator_tag,
                    is_public,
                    item_count: item_count.unwrap_or(0),
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(CollectionListResponse {
        items,
        total,
        limit: safe_limit,
        offset: safe_offset,
    }))
}

pub async fn create_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Require authentication — creator_tag is derived from the authenticated user
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Collection name required".into()));
    }

    // Derive creator_tag from the authenticated user's display_name or id
    let creator_tag: Option<String> = sqlx::query_scalar(
        "SELECT display_name FROM users WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&claims.sub).map_err(|_| AppError::Forbidden("Invalid token".into()))?)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .flatten();

    let creator_tag = creator_tag.unwrap_or_else(|| claims.sub.clone());

    let id = Uuid::now_v7();
    let user_id = uuid::Uuid::parse_str(&claims.sub).map_err(|_| AppError::Forbidden("Invalid token".into()))?;
    sqlx::query(
        "INSERT INTO collections (id, name, description, creator_tag, owner_user_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(&name)
    .bind(&body.description)
    .bind(&creator_tag)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| {
        // If owner_user_id column doesn't exist yet, fall back without it
        if e.to_string().contains("owner_user_id") {
            return AppError::Internal("Database schema out of date — run migrations".into());
        }
        AppError::Internal(e.to_string())
    })?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "name": name, "creator_tag": creator_tag })),
    ))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<CollectionItemsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let safe_limit = params.limit.clamp(1, 200);
    let safe_offset = params.offset.max(0);

    let collection: CollectionRow = sqlx::query_as(
        "SELECT id, name, description, creator_tag, is_public, created_at FROM collections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Collection not found".into()))?;

    let items: Vec<(Uuid, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT l.id, l.image_url, l.thumbnail_small, l.detected_text, l.contributor_tag
         FROM collection_items ci
         JOIN letterings l ON l.id = ci.lettering_id
         WHERE ci.collection_id = $1
         ORDER BY l.created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(safe_limit)
    .bind(safe_offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    let total_items: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM collection_items WHERE collection_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "id": collection.id,
        "name": collection.name,
        "description": collection.description,
        "creator_tag": collection.creator_tag,
        "is_public": collection.is_public,
        "created_at": collection.created_at,
        "total_items": total_items,
        "limit": safe_limit,
        "offset": safe_offset,
        "items": items.into_iter().map(|(id, image_url, thumbnail, detected_text, contributor_tag)| serde_json::json!({
            "id": id,
            "image_url": image_url,
            "thumbnail": thumbnail,
            "detected_text": detected_text,
            "contributor_tag": contributor_tag,
        })).collect::<Vec<_>>(),
    })))
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Path((collection_id, lettering_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token".into()))?;

    // Verify caller owns this collection
    let owner: Option<Option<uuid::Uuid>> = sqlx::query_scalar(
        "SELECT owner_user_id FROM collections WHERE id = $1",
    )
    .bind(collection_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    match owner {
        None => return Err(AppError::NotFound("Collection not found".into())),
        Some(Some(owner_id)) if owner_id != user_id => {
            return Err(AppError::Forbidden("You do not own this collection".into()))
        }
        _ => {}
    }

    sqlx::query("INSERT INTO collection_items (collection_id, lettering_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(collection_id)
        .bind(lettering_id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

pub async fn remove_from_collection(
    State(state): State<AppState>,
    Path((collection_id, lettering_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token".into()))?;

    // Verify caller owns this collection
    let owner: Option<Option<uuid::Uuid>> = sqlx::query_scalar(
        "SELECT owner_user_id FROM collections WHERE id = $1",
    )
    .bind(collection_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    match owner {
        None => return Err(AppError::NotFound("Collection not found".into())),
        Some(Some(owner_id)) if owner_id != user_id => {
            return Err(AppError::Forbidden("You do not own this collection".into()))
        }
        _ => {}
    }

    sqlx::query("DELETE FROM collection_items WHERE collection_id = $1 AND lettering_id = $2")
        .bind(collection_id)
        .bind(lettering_id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// --- Challenges ---

#[derive(Serialize, sqlx::FromRow)]
pub struct Challenge {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub target_script: Option<String>,
    pub target_area: Option<String>,
    pub target_count: i32,
    pub current_count: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_challenges(
    State(state): State<AppState>,
) -> Result<Json<Vec<Challenge>>, AppError> {
    let rows: Vec<Challenge> = sqlx::query_as(
        "SELECT id, title, description, target_script, target_area, target_count, current_count, status, created_at, ends_at FROM challenges WHERE status = 'ACTIVE' ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    Ok(Json(rows))
}
