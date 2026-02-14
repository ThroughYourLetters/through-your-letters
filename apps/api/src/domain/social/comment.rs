use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::ipnetwork::IpNetwork;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Comment {
    pub id: Uuid,
    pub lettering_id: Uuid,
    pub content: String,
    pub user_id: Option<Uuid>,
    pub commenter_name: Option<String>,
    pub status: String,
    pub moderation_score: i32,
    pub moderation_flags: Vec<String>,
    pub auto_flagged: bool,
    pub needs_review: bool,
    pub review_priority: i32,
    #[ts(skip)]
    pub user_ip: Option<IpNetwork>,
    pub moderated_at: Option<DateTime<Utc>>,
    pub moderated_by: Option<String>,
    pub moderation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentModerationInput {
    pub status: String,
    pub moderation_score: i32,
    pub moderation_flags: Vec<String>,
    pub auto_flagged: bool,
    pub needs_review: bool,
    pub review_priority: i32,
    pub moderated_by: Option<String>,
    pub moderation_reason: Option<String>,
}
