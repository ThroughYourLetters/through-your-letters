use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::ipnetwork::IpNetwork;
use ts_rs::TS;
use uuid::Uuid;

/// Core domain entity representing a lettering/typography submission.
///
/// A lettering captures visual text found in public spaces, along with its
/// geographic location, contributor information, and processing metadata.
/// Each lettering undergoes moderation before public visibility.
///
/// # Lifecycle
/// 1. **Uploaded** - Initial submission with basic metadata
/// 2. **Pending** - Awaiting moderation review and ML processing
/// 3. **Approved** - Publicly discoverable and searchable
/// 4. **Rejected** - Hidden from public view with reason
/// 5. **Reported** - Flagged by community for review
///
/// # Invariants
/// - `id` must be unique across all letterings
/// - `location` coordinates must be valid longitude/latitude pairs
/// - `pin_code` must follow regional formatting rules
/// - `contributor_tag` identifies the submitter (may be pseudonymous)
/// - Image URLs must point to accessible storage locations
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct Lettering {
    /// Unique identifier for this lettering entity
    pub id: Uuid,

    /// Reference to the city/region where this lettering was found
    pub city_id: Uuid,

    /// Contributor's chosen display name or tag (may be pseudonymous)
    pub contributor_tag: String,

    /// URL to the full-resolution image stored in persistent storage
    pub image_url: String,

    /// Collection of thumbnail URLs for different display contexts
    pub thumbnail_urls: ThumbnailUrls,

    /// Geographic coordinates where the lettering was photographed
    pub location: Coordinates,

    /// Local postal/zip code for geographic clustering and discovery
    pub pin_code: String,

    /// Machine-extracted text content from OCR processing (optional)
    pub detected_text: Option<String>,

    /// ML-derived metadata about visual characteristics (optional)
    pub ml_metadata: Option<ImageMetadata>,

    /// Human-provided description or story context (optional)
    pub description: Option<String>,

    /// Whether ML analysis confirmed this contains readable text
    pub is_lettering: bool,

    /// Current moderation and visibility status
    pub status: LetteringStatus,

    /// Number of user likes/favorites (cached for performance)
    pub likes_count: i32,

    /// Number of associated comments (cached for performance)
    pub comments_count: i32,

    /// IP address of the uploader (for abuse prevention, not exported to frontend)
    #[ts(skip)]
    pub uploaded_by_ip: Option<IpNetwork>,

    /// Content-based hash for duplicate detection (optional)
    pub image_hash: Option<String>,

    /// Number of community reports filed (cached for moderation)
    pub report_count: i32,

    /// Reasons provided in community reports
    pub report_reasons: Vec<String>,

    /// Additional cultural or historical context (optional)
    pub cultural_context: Option<String>,

    /// Timestamp when this lettering was first uploaded
    pub created_at: DateTime<Utc>,

    /// Timestamp of the most recent modification
    pub updated_at: DateTime<Utc>,
}

/// Collection of thumbnail image URLs for responsive display contexts.
///
/// Thumbnails are pre-generated at upload time to optimize loading performance
/// across different UI components and screen sizes.
///
/// # Size Guidelines
/// - `small`: 200px width for map markers, grid previews
/// - `medium`: 600px width for gallery cards, search results
/// - `large`: 1200px width for detail views, full-screen display
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct ThumbnailUrls {
    /// Small thumbnail (200px) for compact displays and map markers
    pub small: String,

    /// Medium thumbnail (600px) for gallery cards and search results
    pub medium: String,

    /// Large thumbnail (1200px) for detail views and zine-style display
    pub large: String,
}

/// GeoJSON-compliant coordinate representation for geographic locations.
///
/// Follows the GeoJSON Point specification with longitude/latitude ordering.
/// Used for spatial queries, map display, and geographic clustering.
///
/// # Format
/// - `type`: Always "Point" for single location coordinates
/// - `coordinates`: [longitude, latitude] in decimal degrees (WGS84)
///
/// # Example
/// ```json
/// {
///   "type": "Point",
///   "coordinates": [77.5946, 12.9716]  // Bangalore, India
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct Coordinates {
    /// GeoJSON geometry type, always "Point" for lettering locations
    pub r#type: String,

    /// Coordinate pair: [longitude, latitude] in decimal degrees
    pub coordinates: Vec<f64>,
}

impl Coordinates {
    /// Creates a new Point coordinate from longitude and latitude.
    ///
    /// # Arguments
    /// * `longitude` - Longitude in decimal degrees (-180 to 180)
    /// * `latitude` - Latitude in decimal degrees (-90 to 90)
    ///
    /// # Returns
    /// GeoJSON-compliant coordinate structure
    pub fn new_point(longitude: f64, latitude: f64) -> Self {
        Self {
            r#type: "Point".to_string(),
            coordinates: vec![longitude, latitude],
        }
    }

    /// Validates that coordinates are within valid geographic bounds.
    ///
    /// # Returns
    /// `true` if longitude is in [-180, 180] and latitude is in [-90, 90]
    pub fn is_valid(&self) -> bool {
        if self.coordinates.len() != 2 {
            return false;
        }
        let lng = self.coordinates[0];
        let lat = self.coordinates[1];
        lng >= -180.0 && lng <= 180.0 && lat >= -90.0 && lat <= 90.0
    }

    /// Returns the longitude component.
    pub fn longitude(&self) -> Option<f64> {
        self.coordinates.first().copied()
    }

    /// Returns the latitude component.
    pub fn latitude(&self) -> Option<f64> {
        self.coordinates.get(1).copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImageMetadata {
    pub style: Option<String>,
    pub script: Option<String>,
    pub confidence: Option<f32>,
    pub color_palette: Option<Vec<String>>,
}

/// Moderation and visibility status for lettering entities.
///
/// Controls public discoverability and determines which workflows
/// are available for administrators and contributors.
#[derive(Debug, Clone, Serialize, Deserialize, TS, sqlx::Type, Default, PartialEq)]
#[sqlx(type_name = "text", rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum LetteringStatus {
    /// Initial state after upload, awaiting moderation review
    #[default]
    Pending,

    /// Approved for public discovery and search results
    Approved,

    /// Hidden from public view due to policy violations or quality issues
    Rejected,

    /// Flagged by community reports, requires admin attention
    Reported,
}

impl LetteringStatus {
    /// Returns true if this status allows public visibility.
    pub fn is_public(&self) -> bool {
        matches!(self, LetteringStatus::Approved)
    }

    /// Returns true if this status requires administrator attention.
    pub fn needs_moderation(&self) -> bool {
        matches!(self, LetteringStatus::Pending | LetteringStatus::Reported)
    }
}
