use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Contributor {
    pub tag: String,
    pub uploads_count: i32,
    pub likes_received: i32,
    pub joined_at: DateTime<Utc>,
}
