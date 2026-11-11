use crate::presentation::http::pagination::{
    CursorQuery, Page, TimePosition, continuation, decode_cursor, next_link,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
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

fn default_limit() -> i64 {
    50
}
fn default_items_limit() -> i64 {
    100
}

use crate::presentation::http::{
    errors::AppError, middleware::user::decode_required_user_claims, state::AppState,
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
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT contributor_tag, COUNT(*)::bigint, COALESCE(SUM(likes_count), 0)::bigint FROM discoverable_letterings GROUP BY contributor_tag ORDER BY COUNT(*) DESC, contributor_tag ASC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    Ok(Json(
        rows.into_iter()
            .map(|(tag, count, total_likes)| LeaderboardEntry {
                tag,
                count,
                total_likes,
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

#[derive(Serialize, sqlx::FromRow, ts_rs::TS)]
pub struct CollectionResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_tag: String,
    pub is_public: bool,
    #[ts(type = "number")]
    pub item_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, ts_rs::TS)]
pub struct CollectionListResponse {
    pub items: Vec<CollectionResponse>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
}

pub async fn list_collections(
    State(state): State<AppState>,
    Query(params): Query<ListCollectionsQuery>,
) -> Result<Json<CollectionListResponse>, AppError> {
    load_collections(&state, params, None).await
}

pub async fn list_my_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListCollectionsQuery>,
) -> Result<Json<CollectionListResponse>, AppError> {
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;
    let owner = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;
    load_collections(&state, params, Some(owner)).await
}

async fn load_collections(
    state: &AppState,
    params: ListCollectionsQuery,
    owner: Option<Uuid>,
) -> Result<Json<CollectionListResponse>, AppError> {
    let page = crate::presentation::http::pagination::Page::new(
        params.limit,
        params.offset,
        100,
        &state.config.limits.pagination,
    )?;

    let safe_limit = page.limit();
    let safe_offset = page.offset();

    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT c.id, c.name, c.description, c.creator_tag, c.is_public, c.created_at, COUNT(l.id)::bigint AS item_count
         FROM (SELECT id, name, description, creator_tag, is_public, created_at
               FROM collections WHERE ",
    );
    apply_collection_scope(&mut query, owner);
    query
        .push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(safe_limit)
        .push(" OFFSET ")
        .push_bind(safe_offset)
        .push(
            ") c
         LEFT JOIN collection_items ci ON ci.collection_id = c.id
         LEFT JOIN discoverable_letterings l ON l.id = ci.lettering_id
         GROUP BY c.id, c.name, c.description, c.creator_tag, c.is_public, c.created_at
         ORDER BY c.created_at DESC, c.id DESC",
        );
    let items = query
        .build_query_as::<CollectionResponse>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::database)?;

    let mut count =
        QueryBuilder::<Postgres>::new("SELECT COUNT(*)::bigint FROM collections WHERE ");
    apply_collection_scope(&mut count, owner);
    let total = count
        .build_query_scalar::<i64>()
        .fetch_one(&state.db)
        .await
        .map_err(AppError::database)?;

    Ok(Json(CollectionListResponse {
        items,
        total,
        limit: safe_limit,
        offset: safe_offset,
    }))
}

fn apply_collection_scope(query: &mut QueryBuilder<'_, Postgres>, owner: Option<Uuid>) {
    if let Some(owner) = owner {
        query.push("owner_user_id = ").push_bind(owner);
    } else {
        query.push("is_public = true");
    }
}

#[derive(Serialize, ts_rs::TS)]
pub struct CreateCollectionResponse {
    pub id: Uuid,
    pub name: String,
    pub creator_tag: String,
}

pub async fn create_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<CreateCollectionResponse>), AppError> {
    // Require authentication — creator_tag is derived from the authenticated user
    let claims = decode_required_user_claims(&headers, &state.config.jwt_secret)?;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Collection name required".into()));
    }

    // Derive creator_tag from the authenticated user's display_name or id
    let creator_tag: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
            .bind(
                uuid::Uuid::parse_str(&claims.sub)
                    .map_err(|_| AppError::Forbidden("Invalid token".into()))?,
            )
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
            .flatten();

    let creator_tag = creator_tag.unwrap_or_else(|| claims.sub.clone());

    let id = Uuid::now_v7();
    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Forbidden("Invalid token".into()))?;
    sqlx::query(
        "INSERT INTO collections (id, name, description, creator_tag, owner_user_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(&name)
    .bind(&body.description)
    .bind(&creator_tag)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateCollectionResponse {
            id,
            name,
            creator_tag,
        }),
    ))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<CollectionItemsQuery>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = crate::presentation::http::pagination::Page::new(
        params.limit,
        params.offset,
        200,
        &state.config.limits.pagination,
    )?;

    let safe_limit = page.limit();
    let safe_offset = page.offset();

    let requester = crate::presentation::http::middleware::user::decode_optional_user_claims(
        &headers,
        &state.config.jwt_secret,
    )
    .and_then(|c| Uuid::parse_str(&c.sub).ok());
    let collection: CollectionRow = sqlx::query_as(
        "SELECT id, name, description, creator_tag, is_public, created_at FROM collections WHERE id = $1 AND (is_public OR owner_user_id = $2)",
    )
    .bind(id)
    .bind(requester)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?
    .ok_or_else(|| AppError::NotFound("Collection not found".into()))?;

    let items: Vec<(Uuid, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT l.id, l.image_url, l.thumbnail_small, l.detected_text, l.contributor_tag
         FROM collection_items ci
         JOIN discoverable_letterings l ON l.id = ci.lettering_id
         WHERE ci.collection_id = $1
         ORDER BY l.created_at DESC, l.id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(safe_limit)
    .bind(safe_offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    let total_items: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM collection_items ci JOIN discoverable_letterings l ON l.id = ci.lettering_id WHERE ci.collection_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

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
    let owner: Option<Option<uuid::Uuid>> =
        sqlx::query_scalar("SELECT owner_user_id FROM collections WHERE id = $1")
            .bind(collection_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    match owner {
        None => return Err(AppError::NotFound("Collection not found".into())),
        Some(owner_id) if owner_id != Some(user_id) => {
            return Err(AppError::Forbidden("You do not own this collection".into()));
        }
        _ => {}
    }

    let result = sqlx::query("INSERT INTO collection_items (collection_id, lettering_id) SELECT $1, id FROM readable_letterings($3) WHERE id = $2 ON CONFLICT DO NOTHING")
        .bind(collection_id)
        .bind(lettering_id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    if result.rows_affected() == 0 {
        let readable: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM readable_letterings($2) WHERE id = $1)",
        )
        .bind(lettering_id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
        if !readable {
            return Err(AppError::NotFound("Lettering not found".into()));
        }
    }

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
    let owner: Option<Option<uuid::Uuid>> =
        sqlx::query_scalar("SELECT owner_user_id FROM collections WHERE id = $1")
            .bind(collection_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    match owner {
        None => return Err(AppError::NotFound("Collection not found".into())),
        Some(owner_id) if owner_id != Some(user_id) => {
            return Err(AppError::Forbidden("You do not own this collection".into()));
        }
        _ => {}
    }

    sqlx::query("DELETE FROM collection_items WHERE collection_id = $1 AND lettering_id = $2")
        .bind(collection_id)
        .bind(lettering_id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

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
    Query(query): Query<CursorQuery>,
) -> Result<(HeaderMap, Json<Vec<Challenge>>), AppError> {
    let page = Page::new(
        query.limit.unwrap_or(100),
        0,
        100,
        &state.config.limits.pagination,
    )?;
    let limit = page.limit();
    let scope = "challenges:active";
    let after: Option<TimePosition> = decode_cursor(
        query.after.as_deref(),
        scope,
        &state.config.limits.pagination,
    )?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT id, title, description, target_script, target_area, target_count, current_count, status, created_at, ends_at FROM challenges WHERE status = 'ACTIVE'",
    );
    if let Some(after) = after {
        sql.push(" AND (created_at,id) < (")
            .push_bind(after.created_at)
            .push(",")
            .push_bind(after.id)
            .push(")");
    }
    sql.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<Challenge> = sql
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| AppError::Internal(anyhow::anyhow!(e)))?;

    let next = continuation(&mut rows, page, scope, |row| TimePosition {
        created_at: row.created_at,
        id: row.id,
    })?;
    let headers = next_link(
        "/api/v1/challenges",
        &[("limit", limit.to_string())],
        next.as_deref(),
    )?;
    Ok((headers, Json(rows)))
}
