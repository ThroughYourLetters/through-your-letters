use bytes::Bytes;
use sqlx::types::ipnetwork::IpNetwork;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UploadLetteringRequest {
    pub city_id: Uuid,
    pub contributor_tag: String,
    pub pin_code: String,
    pub image_data: Bytes,
    pub description: Option<String>,
    pub uploaded_by_ip: Option<IpNetwork>,
}
