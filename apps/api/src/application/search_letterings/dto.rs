use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<i64>,
}
