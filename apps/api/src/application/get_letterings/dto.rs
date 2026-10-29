use crate::domain::lettering::entity::Lettering;
#[derive(Debug)]
pub struct PaginatedResponse {
    pub letterings: Vec<Lettering>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}
