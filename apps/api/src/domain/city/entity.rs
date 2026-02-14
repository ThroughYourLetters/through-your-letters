use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct City {
    pub id: Uuid,
    pub name: String,
    pub country_code: String,
    pub created_at: DateTime<Utc>,
}
