use anyhow::Result;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct VirusScanner {
    enabled: bool,
    host: String,
    port: u16,
}

impl VirusScanner {
    pub fn new(enabled: bool, host: Option<String>, port: Option<u16>) -> Self {
        Self {
            enabled,
            host: host.unwrap_or_else(|| "clamav".to_string()),
            port: port.unwrap_or(3310),
        }
    }

    pub async fn scan(&self, data: &Bytes) -> Result<bool> {
        if !self.enabled {
            return Ok(true);
        }

        let mut stream = match TcpStream::connect(format!("{}:{}", self.host, self.port)).await {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!("clamav unavailable: {}", err);
                return Ok(true);
            }
        };

        if let Err(err) = stream.write_all(b"zINSTREAM\0").await {
            tracing::warn!("clamav write failed: {}", err);
            return Ok(true);
        }
        let len = data.len() as u32;
        if let Err(err) = stream.write_all(&len.to_be_bytes()).await {
            tracing::warn!("clamav write length failed: {}", err);
            return Ok(true);
        }
        if let Err(err) = stream.write_all(data).await {
            tracing::warn!("clamav write data failed: {}", err);
            return Ok(true);
        }
        if let Err(err) = stream.write_all(&0u32.to_be_bytes()).await {
            tracing::warn!("clamav write terminator failed: {}", err);
            return Ok(true);
        }
        if let Err(err) = stream.flush().await {
            tracing::warn!("clamav flush failed: {}", err);
            return Ok(true);
        }

        let mut response = String::new();
        if let Err(err) = stream.read_to_string(&mut response).await {
            tracing::warn!("clamav read failed: {}", err);
            return Ok(true);
        }

        if response.contains("OK") {
            return Ok(true);
        }
        if response.contains("FOUND") {
            return Ok(false);
        }
        tracing::warn!("clamav unexpected response: {}", response.trim());
        Ok(true)
    }
}
