use crate::domain::lettering::{entity::*, errors::DomainError, repository::LetteringRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, types::ipnetwork::IpNetwork};
use tracing::{error, info, debug, instrument};
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
    location_wkt: String,
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
            .strip_prefix("POINT(")
            .and_then(|s| s.strip_suffix(")"))
            .and_then(|s| s.split_once(' '))
            .and_then(|(lng, lat)| Some(vec![lng.parse().ok()?, lat.parse().ok()?]))
            .unwrap_or_else(|| vec![0.0, 0.0]);

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
            location: Coordinates {
                r#type: "Point".into(),
                coordinates: coords,
            },
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
impl SqlxLetteringRepository {
    /// Creates a new instance of the repository with the provided database pool.
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool for database operations
    pub fn new(pool: PgPool) -> Self {
        info!("Initializing SqlxLetteringRepository with connection pool");
        Self { pool }
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
    #[instrument(skip(self), fields(query_len = query.len(), limit = limit))]
    pub async fn search_with_locale(
        &self,
        query: &str,
        locale: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Lettering>, DomainError> {
        debug!("Starting search with query: '{}', locale: {:?}", query, locale);

        let ts_config = Self::ts_config_for_locale(locale);
        let like = format!("%{}%", query);
        let safe_limit = limit.clamp(1, 100);

        debug!("Using text search config: {}, safe_limit: {}", ts_config, safe_limit);

        let rows = sqlx::query_as::<_, LetteringRow>(
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large,
                      pin_code, status, created_at, updated_at, likes_count, comments_count,
                      detected_text, description, image_hash, report_count, report_reasons, cultural_context,
                      ml_style, ml_script, ml_confidence, ml_color_palette,
                      ST_AsText(location) AS location_wkt, uploaded_by_ip
               FROM letterings
               WHERE status = 'APPROVED'
                 AND COALESCE((
                     SELECT rp.discoverability_enabled
                     FROM cities c
                     LEFT JOIN region_policies rp ON rp.country_code = c.country_code
                     WHERE c.id = letterings.city_id
                 ), true)
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
            error!("Search query failed: {}", e);
            DomainError::InfrastructureError(format!("Search operation failed: {}", e))
        })?;

        let result_count = rows.len();
        debug!("Search completed successfully, found {} results", result_count);

        Ok(rows.into_iter().map(Lettering::from).collect())
    }
}

#[async_trait]
impl LetteringRepository for SqlxLetteringRepository {
    /// Creates a new lettering entity in the database.
    ///
    /// This method inserts a lettering with all associated metadata including
    /// location data, thumbnails, and ML-detected attributes. The entity
    /// is initially set to PENDING status for moderation review.
    ///
    /// # Arguments
    /// * `l` - Lettering entity to persist
    ///
    /// # Returns
    /// The created lettering entity with database-assigned fields
    ///
    /// # Errors
    /// Returns `DomainError::InfrastructureError` for database constraint violations
    /// or connectivity issues
    #[instrument(skip(self, l), fields(lettering_id = %l.id, contributor = %l.contributor_tag))]
    async fn create(&self, l: &Lettering) -> Result<Lettering, DomainError> {
        let pt = format!(
            "POINT({} {})",
            l.location.coordinates[0], l.location.coordinates[1]
        );

        debug!("Creating lettering with location: {}", pt);

        sqlx::query!(
            r#"INSERT INTO letterings (id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, location, pin_code, status, uploaded_by_ip, image_hash, description)
               VALUES ($1, $2, $3, $4, $5, $6, $7, ST_GeogFromText($8), $9, $10, $11, $12, $13)"#,
            l.id, l.city_id, l.contributor_tag, l.image_url, l.thumbnail_urls.small, l.thumbnail_urls.medium, l.thumbnail_urls.large, pt, l.pin_code, "PENDING", l.uploaded_by_ip as _, l.image_hash, l.description
        ).execute(&self.pool).await.map_err(|e| {
            error!("Failed to create lettering {}: {}", l.id, e);
            DomainError::InfrastructureError(format!("Failed to create lettering: {}", e))
        })?;

        info!("Successfully created lettering {} by {}", l.id, l.contributor_tag);
        Ok(l.clone())
    }

    /// Retrieves all approved letterings with pagination support.
    ///
    /// This method fetches letterings that have passed moderation review,
    /// ordered by creation date (newest first).
    ///
    /// # Arguments
    /// * `limit` - Maximum number of letterings to return
    /// * `offset` - Number of letterings to skip (for pagination)
    ///
    /// # Returns
    /// Vector of approved lettering entities
    #[instrument(skip(self))]
    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<Lettering>, DomainError> {
        let rows = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, ST_AsText(location) as "location_wkt!", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE status = 'APPROVED' ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
            limit, offset
        ).fetch_all(&self.pool).await.map_err(|e| {
            error!("Failed to fetch letterings with limit {} offset {}: {}", limit, offset, e);
            DomainError::InfrastructureError(format!("Failed to retrieve letterings: {}", e))
        })?;

        debug!("Retrieved {} letterings", rows.len());
        Ok(rows.into_iter().map(Lettering::from).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Lettering>, DomainError> {
        let row = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, ST_AsText(location) as "location_wkt!", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE id = $1"#, id
        ).fetch_optional(&self.pool).await.map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(row.map(Lettering::from))
    }

    async fn find_by_image_hash(&self, hash: &str) -> Result<Option<Lettering>, DomainError> {
        let row = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, ST_AsText(location) as "location_wkt!", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE image_hash = $1"#, hash
        ).fetch_optional(&self.pool).await.map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(row.map(Lettering::from))
    }

    async fn search(&self, q: &str) -> Result<Vec<Lettering>, DomainError> {
        self.search_with_locale(q, Some("en"), 50).await
    }

    async fn count_by_contributor_today(&self, tag: &str) -> Result<i64, DomainError> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM letterings WHERE contributor_tag = $1 AND created_at > CURRENT_DATE", tag)
            .fetch_one(&self.pool).await.map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(count.unwrap_or(0))
    }

    async fn find_by_contributor(
        &self,
        tag: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Lettering>, DomainError> {
        let rows = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, ST_AsText(location) as "location_wkt!", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE contributor_tag = $1 AND status = 'APPROVED' ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
            tag, limit, offset
        ).fetch_all(&self.pool).await.map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(rows.into_iter().map(Lettering::from).collect())
    }

    async fn count_by_contributor(&self, tag: &str) -> Result<i64, DomainError> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM letterings WHERE contributor_tag = $1 AND status = 'APPROVED'",
            tag
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(count.unwrap_or(0))
    }

    async fn find_by_city(
        &self,
        city_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Lettering>, DomainError> {
        let rows = sqlx::query_as!(LetteringRow,
            r#"SELECT id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large, pin_code, status, created_at, updated_at, likes_count, comments_count, detected_text, description, image_hash, report_count, report_reasons, cultural_context, ml_style, ml_script, ml_confidence, ml_color_palette, ST_AsText(location) as "location_wkt!", uploaded_by_ip as "uploaded_by_ip: _" FROM letterings WHERE city_id = $1 AND status = 'APPROVED' ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
            city_id, limit, offset
        ).fetch_all(&self.pool).await.map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        Ok(rows.into_iter().map(Lettering::from).collect())
    }

    async fn update(&self, l: &Lettering) -> Result<Lettering, DomainError> {
        if l.location.coordinates.len() < 2 {
            return Err(DomainError::ValidationError(
                "location must include longitude and latitude".into(),
            ));
        }

        let point = format!(
            "POINT({} {})",
            l.location.coordinates[0], l.location.coordinates[1]
        );
        let status = match l.status {
            LetteringStatus::Pending => "PENDING",
            LetteringStatus::Approved => "APPROVED",
            LetteringStatus::Rejected => "REJECTED",
            LetteringStatus::Reported => "REPORTED",
        };
        let report_reasons = serde_json::to_value(&l.report_reasons)
            .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;
        let color_palette_json = l
            .ml_metadata
            .as_ref()
            .and_then(|m| m.color_palette.clone())
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;

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
                   likes_count = $21,
                   comments_count = $22,
                   uploaded_by_ip = $23,
                   updated_at = NOW()
               WHERE id = $1
               RETURNING id, city_id, contributor_tag, image_url, thumbnail_small, thumbnail_medium, thumbnail_large,
                         pin_code, status, created_at, updated_at, likes_count, comments_count,
                         detected_text, description, image_hash, report_count, report_reasons, cultural_context,
                         ml_style, ml_script, ml_confidence, ml_color_palette,
                         ST_AsText(location) AS location_wkt, uploaded_by_ip"#,
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
        .bind(l.likes_count)
        .bind(l.comments_count)
        .bind(l.uploaded_by_ip.clone())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError(e.to_string()))?;

        let updated = row.ok_or_else(|| DomainError::NotFound("Lettering not found".into()))?;
        Ok(updated.into())
    }
    /// Permanently deletes a lettering entity from the database.
    ///
    /// This operation is irreversible and will cascade to related entities
    /// such as comments and likes. Use with caution.
    ///
    /// # Arguments
    /// * `id` - UUID of the lettering to delete
    ///
    /// # Errors
    /// Returns `DomainError::InfrastructureError` if the deletion fails
    #[instrument(skip(self), fields(lettering_id = %id))]
    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!("DELETE FROM letterings WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete lettering {}: {}", id, e);
                DomainError::InfrastructureError(format!("Failed to delete lettering: {}", e))
            })?;

        if result.rows_affected() == 0 {
            debug!("No lettering found with id {} for deletion", id);
        } else {
            info!("Successfully deleted lettering {}", id);
        }

        Ok(())
    }
}
