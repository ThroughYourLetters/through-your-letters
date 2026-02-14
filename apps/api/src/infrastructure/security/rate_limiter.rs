use redis::{AsyncCommands, Client};

pub struct RateLimiter {
    client: Client,
}
impl RateLimiter {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
    pub async fn check(&self, key: &str, limit: u32) -> bool {
        if let Ok(mut conn) = self.client.get_multiplexed_async_connection().await {
            let k = format!("rl:{}", key);
            let count: u32 = conn.incr(&k, 1).await.unwrap_or(0);
            if count == 1 {
                let _: () = conn.expire(&k, 3600).await.unwrap_or(());
            }
            count <= limit
        } else {
            true
        }
    }
}
