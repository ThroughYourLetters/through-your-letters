use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AddCommentRequest {
    pub lettering_id: Uuid,
    pub content: String,
}
