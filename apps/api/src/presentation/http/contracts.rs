//! JSON allowlists. Domain/database fields never become response fields implicitly.
//! Owner and administrator contracts live beside their authenticated handlers;
//! `generate_contracts` exports those same Rust types for the web client.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::domain::{
    lettering::entity::{
        Coordinates, ImageMetadata, Lettering, LetteringStatus, LocationSource, ThumbnailUrls,
    },
    social::comment::Comment,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublicLetteringStatus {
    Pending,
    Approved,
    Rejected,
    Reported,
}

impl From<LetteringStatus> for PublicLetteringStatus {
    fn from(status: LetteringStatus) -> Self {
        match status {
            LetteringStatus::Pending => Self::Pending,
            LetteringStatus::Approved => Self::Approved,
            LetteringStatus::Rejected => Self::Rejected,
            LetteringStatus::Reported => Self::Reported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PublicThumbnailUrls {
    pub small: String,
    pub medium: String,
    pub large: String,
}

impl From<ThumbnailUrls> for PublicThumbnailUrls {
    fn from(value: ThumbnailUrls) -> Self {
        Self {
            small: value.small,
            medium: value.medium,
            large: value.large,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PublicCoordinates {
    #[ts(type = "\"Point\"")]
    pub r#type: String,
    pub coordinates: [f64; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PublicImageMetadata {
    pub style: Option<String>,
    pub script: Option<String>,
    pub confidence: Option<f32>,
    pub color_palette: Option<Vec<String>>,
}

impl From<ImageMetadata> for PublicImageMetadata {
    fn from(value: ImageMetadata) -> Self {
        Self {
            style: value.style,
            script: value.script,
            // Legacy heuristic scores have no calibrated statistical meaning.
            confidence: None,
            color_palette: value.color_palette,
        }
    }
}

/// Content safe for discovery, search and detail responses, including owner reads.
/// Abuse evidence, IPs, hashes and provider errors stay on the server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PublicLettering {
    pub id: Uuid,
    pub city_id: Uuid,
    pub contributor_tag: String,
    pub image_url: String,
    pub thumbnail_urls: PublicThumbnailUrls,
    pub location: Option<PublicCoordinates>,
    pub location_source: LocationSource,
    pub pin_code: String,
    pub detected_text: Option<String>,
    pub ml_metadata: Option<PublicImageMetadata>,
    pub description: Option<String>,
    pub is_lettering: bool,
    pub status: PublicLetteringStatus,
    pub likes_count: i32,
    pub comments_count: i32,
    pub cultural_context: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Lettering> for PublicLettering {
    fn from(value: Lettering) -> Self {
        Self {
            id: value.id,
            city_id: value.city_id,
            contributor_tag: value.contributor_tag,
            image_url: value.image_url,
            thumbnail_urls: value.thumbnail_urls.into(),
            location: value
                .location
                .filter(Coordinates::is_valid)
                .and_then(|point| <[f64; 2]>::try_from(point.coordinates).ok())
                .map(|coordinates| PublicCoordinates {
                    r#type: "Point".into(),
                    coordinates,
                }),
            location_source: value.location_source,
            pin_code: value.pin_code,
            detected_text: value.detected_text,
            ml_metadata: value.ml_metadata.map(Into::into),
            description: value.description,
            is_lettering: value.is_lettering,
            status: value.status.into(),
            likes_count: value.likes_count,
            comments_count: value.comments_count,
            cultural_context: value.cultural_context,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct LetteringDetailResponse {
    #[serde(flatten)]
    pub lettering: PublicLettering,
    pub is_owner: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct GalleryResponse {
    pub letterings: Vec<PublicLettering>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub limit: i64,
    #[ts(type = "number")]
    pub offset: i64,
}

/// Comment creation also returns this allowlist, even when moderation hides it.
#[derive(Debug, Serialize, TS)]
pub struct PublicComment {
    pub id: Uuid,
    pub lettering_id: Uuid,
    pub content: String,
    pub commenter_name: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Comment> for PublicComment {
    fn from(value: Comment) -> Self {
        Self {
            id: value.id,
            lettering_id: value.lettering_id,
            content: value.content,
            commenter_name: value.commenter_name,
            status: value.status,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
