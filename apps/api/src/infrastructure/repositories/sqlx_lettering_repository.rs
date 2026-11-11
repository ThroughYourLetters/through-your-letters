use crate::domain::lettering::{
    entity::*,
    errors::DomainError,
    repository::{LetteringRepository, NewUpload},
};
use crate::infrastructure::queue::postgres_jobs::PostgresJobs;
use crate::infrastructure::storage::upload_attempts::PostgresUploadAttempts;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, types::ipnetwork::IpNetwork};
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

#[derive(FromRow)]
struct LetteringRow {
    id: Uuid,
    city_id: Uuid,
    contributor_tag: String,
    image_url: String,
    thumbnail_small: String,
    thumbnail_medium: String,
    thumbnail_large: String,
    location_wkt: Option<String>,
    location_source: LocationSource,
    location_error: Option<String>,
    pin_code: String,
    status: String,
    uploaded_by_ip: Option<IpNetwork>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    likes_count: i32,
    comments_count: i32,
    detected_text: Option<String>,
    description: Option<String>,
    image_hash: Option<String>,
    report_count: i32,
    report_reasons: serde_json::Value,
    cultural_context: Option<String>,
    ml_style: Option<String>,
    ml_script: Option<String>,
    ml_confidence: Option<f32>,
    ml_color_palette: Option<serde_json::Value>,
}

impl From<LetteringRow> for Lettering {
    fn from(r: LetteringRow) -> Self {
        let coords = r
            .location_wkt
            .as_deref()
            .and_then(|s| s.strip_prefix("POINT("))
            .and_then(|s| s.strip_suffix(")"))
            .and_then(|s| s.split_once(' '))
            .and_then(|(lng, lat)| {
                Some(Coordinates::new_point(lng.parse().ok()?, lat.parse().ok()?))
            })
            .filter(Coordinates::is_valid);

        Lettering {
            id: r.id,
            city_id: r.city_id,
            contributor_tag: r.contributor_tag,
            image_url: r.image_url,
            thumbnail_urls: ThumbnailUrls {
                small: r.thumbnail_small,
                medium: r.thumbnail_medium,
                large: r.thumbnail_large,
            },
            location: coords,
            location_source: r.location_source,
            location_error: r.location_error,
            pin_code: r.pin_code,
            detected_text: r.detected_text,
            ml_metadata: Some(ImageMetadata {
                style: r.ml_style,
                script: r.ml_script,
                confidence: r.ml_confidence,
                color_palette: r
                    .ml_color_palette
                    .and_then(|v| serde_json::from_value(v).ok()),
            }),
            description: r.description,
            is_lettering: true,
            status: match r.status.as_str() {
                "APPROVED" => LetteringStatus::Approved,
                "REJECTED" => LetteringStatus::Rejected,
                "REPORTED" => LetteringStatus::Reported,
                _ => LetteringStatus::Pending,
            },
            likes_count: r.likes_count,
            comments_count: r.comments_count,
            uploaded_by_ip: r.uploaded_by_ip,
            image_hash: r.image_hash,
            report_count: r.report_count,
            report_reasons: serde_json::from_value(r.report_reasons).unwrap_or_default(),
            cultural_context: r.cultural_context,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub struct SqlxLetteringRepository {
    pub pool: PgPool,
}

#[derive(sqlx::FromRow)]
pub struct UploadReceipt {
    pub lettering_id: Option<Uuid>,
    pub payload_hash: String,
    pub analysis_requested: bool,
}

impl SqlxLetteringRepository {
    pub async fn find_upload_request(
        &self,
        owner_id: Option<Uuid>,
        key: Uuid,
    ) -> Result<Option<UploadReceipt>, DomainError> {
        sqlx::query_as("SELECT lettering_id,payload_hash,analysis_requested FROM upload_requests WHERE owner_id IS NOT DISTINCT FROM $1 AND request_key=$2")
            .bind(owner_id).bind(key).fetch_optional(&self.pool).await
            .map_err(DomainError::infrastructure)
    }
    /// Creates a new instance of the repository with the provided database pool.
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool for database operations
    pub fn new(pool: PgPool) -> Self {
        info!("Initializing SqlxLetteringRepository with connection pool");
        Self { pool }
    }

    /// Fetch public content or an owner's preview in the same statement as the data.
    pub async fn find_readable_by_id(
        &self,
        id: Uuid,
        requester: Option<Uuid>,
    ) -> Result<Option<Lettering>, DomainError> {
        let row = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, location_source as "location_source: LocationSource", location_error, ST_AsText(location) as "location_wkt?", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE id = $1 AND id IN (SELECT id FROM readable_letterings($2))"#,
            id, requester
        ).fetch_optional(&self.pool).await
            .map_err(DomainError::infrastructure)?;
        Ok(row.map(Lettering::from))
    }

    fn ts_config_for_locale(locale: Option<&str>) -> &'static str {
        let normalized = locale.unwrap_or("en").trim().to_ascii_lowercase();

        if normalized.starts_with("en") {
            "english"
        } else {
            "simple"
        }
    }

    /// Performs locale-aware search across lettering entities.
    ///
    /// This method combines full-text search using PostgreSQL's text search capabilities
    /// with fuzzy matching on contributor tags and descriptions. Results are ranked by
    /// relevance and filtered by approval status.
    ///
    /// # Arguments
    /// * `query` - Search term or phrase
    /// * `locale` - Optional locale for language-specific search configuration
    /// * `limit` - Maximum number of results to return (clamped between 1-100)
    ///
    /// # Returns
    /// Vector of matching lettering entities ordered by relevance
    ///
    /// # Errors
    /// Returns `DomainError::InfrastructureError` for database connectivity issues
    /// or query execution failures
    #[instrument(skip_all, fields(query_len = query.len(), limit = limit))]
    pub async fn search_with_locale(
        &self,
        query: &str,
        locale: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Lettering>, DomainError> {
        debug!(query_length = query.chars().count(), "Starting search");

        let ts_config = Self::ts_config_for_locale(locale);
        let like = format!("%{}%", query);
        let safe_limit = limit.clamp(1, 100);

        debug!(
            "Using text search config: {}, safe_limit: {}",
            ts_config, safe_limit
        );

        let rows = sqlx::query_as::<_, LetteringRow>(
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large,
                      pin_code, status, created_at, updated_at, likes_count, comments_count,
                      detected_text, description, image_hash, report_count, report_reasons, cultural_context,
                      ml_style, ml_script, ml_confidence, ml_color_palette,
                      location_source, location_error, ST_AsText(location) AS location_wkt, uploaded_by_ip
               FROM letterings
               WHERE id IN (SELECT id FROM discoverable_letterings)
                 AND (
                     detected_text_tsv @@ websearch_to_tsquery($1::regconfig, $2)
                     OR detected_text ILIKE $3
                     OR description ILIKE $3
                     OR contributor_tag ILIKE $3
                 )
               ORDER BY likes_count DESC, created_at DESC
               LIMIT $4"#,
        )
        .bind(ts_config)
        .bind(query)
        .bind(like)
        .bind(safe_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!("Search query failed");
            DomainError::InfrastructureError(anyhow::anyhow!(e))
        })?;

        let result_count = rows.len();
        debug!(
            "Search completed successfully, found {} results",
            result_count
        );

        Ok(rows.into_iter().map(Lettering::from).collect())
    }
}

#[async_trait]
impl LetteringRepository for SqlxLetteringRepository {
    /// Commit the upload, ownership, and optional analysis job together.
    #[instrument(skip_all, fields(lettering_id = %upload.lettering.id))]
    async fn create(&self, upload: NewUpload<'_>) -> Result<Lettering, DomainError> {
        let NewUpload {
            lettering: l,
            owner_id,
            analysis_attempts,
            request,
            storage_attempt,
        } = upload;
        let pt = location_wkt(l.location.as_ref())?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(DomainError::infrastructure)?;
        if let Some(owner) = owner_id {
            let active = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM users WHERE id=$1 AND deleted_at IS NULL FOR SHARE",
            )
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await
            .map_err(DomainError::infrastructure)?;
            if active.is_none() {
                return Err(DomainError::Unauthorized);
            }
        }

        if let Some(attempt) = storage_attempt
            && !PostgresUploadAttempts::lock_in(&mut tx, attempt, l)
                .await
                .map_err(DomainError::infrastructure)?
        {
            return Err(DomainError::Conflict(
                "Upload attempt expired or its files changed; retry the upload".into(),
            ));
        }
        sqlx::query!(
            r#"INSERT INTO letterings (id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, location, pin_code, status, uploaded_by_ip, image_hash, description, location_source, location_error, user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, ST_GeogFromText($8), $9, $10, $11, $12, $13, $14, $15, $16)"#,
            l.id, l.city_id, l.contributor_tag, l.image_url, l.thumbnail_urls.small, l.thumbnail_urls.medium, l.thumbnail_urls.large, pt, l.pin_code, "PENDING", l.uploaded_by_ip as _, l.image_hash, l.description, l.location_source as _, l.location_error, owner_id
        ).execute(&mut *tx).await.map_err(|e| {
            error!(lettering_id = %l.id, "Failed to create lettering");
            DomainError::InfrastructureError(anyhow::anyhow!(e))
        })?;

        if let Some(attempts) = analysis_attempts {
            PostgresJobs::enqueue_in(&mut tx, l.id, attempts.get())
                .await
                .map_err(DomainError::infrastructure)?;
        }
        if let Some(request) = request {
            sqlx::query("INSERT INTO upload_requests(id,owner_id,request_key,payload_hash,lettering_id,analysis_requested) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(Uuid::now_v7()).bind(owner_id).bind(request.key).bind(request.payload_hash)
                .bind(l.id).bind(analysis_attempts.is_some()).execute(&mut *tx).await
                .map_err(DomainError::infrastructure)?;
        }
        if let Some(attempt) = storage_attempt
            && !PostgresUploadAttempts::commit_in(&mut tx, attempt)
                .await
                .map_err(DomainError::infrastructure)?
        {
            return Err(DomainError::Conflict(
                "Upload attempt changed before saving; retry the upload".into(),
            ));
        }
        tx.commit().await.map_err(DomainError::infrastructure)?;
        info!(lettering_id = %l.id, "Lettering created");
        Ok(l.clone())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Lettering>, DomainError> {
        let row = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, location_source as "location_source: LocationSource", location_error, ST_AsText(location) as "location_wkt?", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE id = $1"#, id
        ).fetch_optional(&self.pool).await.map_err(DomainError::infrastructure)?;
        Ok(row.map(Lettering::from))
    }

    async fn find_by_image_hash(&self, hash: &str) -> Result<Option<Lettering>, DomainError> {
        let row = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, location_source as "location_source: LocationSource", location_error, ST_AsText(location) as "location_wkt?", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE image_hash = $1"#, hash
        ).fetch_optional(&self.pool).await.map_err(DomainError::infrastructure)?;
        Ok(row.map(Lettering::from))
    }

    async fn find_by_contributor(
        &self,
        tag: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Lettering>, DomainError> {
        let rows = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, location_source as "location_source: LocationSource", location_error, ST_AsText(location) as "location_wkt?", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE contributor_tag = $1 AND id IN (SELECT id FROM discoverable_letterings) ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
            tag, limit, offset
        ).fetch_all(&self.pool).await.map_err(DomainError::infrastructure)?;
        Ok(rows.into_iter().map(Lettering::from).collect())
    }

    async fn count_by_contributor(&self, tag: &str) -> Result<i64, DomainError> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM letterings WHERE contributor_tag = $1 AND id IN (SELECT id FROM discoverable_letterings)",
            tag
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DomainError::infrastructure)?;
        Ok(count.unwrap_or(0))
    }

    async fn update(&self, l: &Lettering) -> Result<Lettering, DomainError> {
        let point = location_wkt(l.location.as_ref())?;
        let status = match l.status {
            LetteringStatus::Pending => "PENDING",
            LetteringStatus::Approved => "APPROVED",
            LetteringStatus::Rejected => "REJECTED",
            LetteringStatus::Reported => "REPORTED",
        };
        let report_reasons =
            serde_json::to_value(&l.report_reasons).map_err(DomainError::infrastructure)?;
        let color_palette_json = l
            .ml_metadata
            .as_ref()
            .and_then(|m| m.color_palette.clone())
            .map(serde_json::to_value)
            .transpose()
            .map_err(DomainError::infrastructure)?;

        let row = sqlx::query_as::<_, LetteringRow>(
            r#"UPDATE letterings
               SET city_id = $2,
                   contributor_tag = $3,
                   image_url = $4,
                   thumbnail_small = $5,
                   thumbnail_medium = $6,
                   thumbnail_large = $7,
                   location = ST_GeogFromText($8),
                   pin_code = $9,
                   detected_text = $10,
                   description = $11,
                   image_hash = $12,
                   status = $13,
                   ml_style = $14,
                   ml_script = $15,
                   ml_confidence = $16,
                   ml_color_palette = COALESCE($17, '[]'::jsonb),
                   cultural_context = $18,
                   report_count = $19,
                   report_reasons = $20,
                   uploaded_by_ip = $21,
                   location_source = $22,
                   location_error = $23,
                   updated_at = NOW()
               WHERE id = $1
               RETURNING id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large,
                         pin_code, status, created_at, updated_at, likes_count, comments_count,
                         detected_text, description, image_hash, report_count, report_reasons, cultural_context,
                         ml_style, ml_script, ml_confidence, ml_color_palette,
                         location_source, location_error, ST_AsText(location) AS location_wkt, uploaded_by_ip"#,
        )
        .bind(l.id)
        .bind(l.city_id)
        .bind(&l.contributor_tag)
        .bind(&l.image_url)
        .bind(&l.thumbnail_urls.small)
        .bind(&l.thumbnail_urls.medium)
        .bind(&l.thumbnail_urls.large)
        .bind(point)
        .bind(&l.pin_code)
        .bind(&l.detected_text)
        .bind(&l.description)
        .bind(&l.image_hash)
        .bind(status)
        .bind(l.ml_metadata.as_ref().and_then(|m| m.style.as_deref()))
        .bind(l.ml_metadata.as_ref().and_then(|m| m.script.as_deref()))
        .bind(l.ml_metadata.as_ref().and_then(|m| m.confidence))
        .bind(color_palette_json)
        .bind(&l.cultural_context)
        .bind(l.report_count)
        .bind(report_reasons)
        .bind(l.uploaded_by_ip)
        .bind(l.location_source)
        .bind(&l.location_error)
        .fetch_optional(&self.pool)
        .await
        .map_err(DomainError::infrastructure)?;

        let updated = row.ok_or_else(|| DomainError::NotFound("Lettering not found".into()))?;
        Ok(updated.into())
    }
}

fn location_wkt(location: Option<&Coordinates>) -> Result<Option<String>, DomainError> {
    location
        .map(|point| {
            if !point.is_valid() {
                return Err(DomainError::ValidationError(
                    "Invalid location coordinates".into(),
                ));
            }
            Ok(format!(
                "POINT({} {})",
                point.coordinates[0], point.coordinates[1]
            ))
        })
        .transpose()
}
