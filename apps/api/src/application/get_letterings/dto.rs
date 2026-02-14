use crate::domain::lettering::entity::Lettering;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PaginatedResponse {
    pub letterings: Vec<Lettering>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}
