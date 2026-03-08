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

    /// Scans data for viruses. Returns `Ok(true)` if clean, `Ok(false)` if infected.
    ///
    /// When scanning is disabled, always returns `Ok(true)` (clean).
    /// When scanning is enabled, fails **closed** — any error reaching or reading ClamAV
    /// returns `Err`, causing the upload to be rejected. This prevents silent bypass when
    /// ClamAV is unreachable.
    pub async fn scan(&self, data: &Bytes) -> Result<bool> {
        if !self.enabled {
            return Ok(true);
        }

        let mut stream =
            TcpStream::connect(format!("{}:{}", self.host, self.port))
                .await
                .map_err(|err| {
                    tracing::error!("clamav unavailable (fail-closed): {}", err);
                    anyhow::anyhow!("Virus scanner unavailable — upload rejected for safety")
                })?;

        stream.write_all(b"zINSTREAM\0").await.map_err(|err| {
            tracing::error!("clamav write failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        let len = data.len() as u32;
        stream.write_all(&len.to_be_bytes()).await.map_err(|err| {
            tracing::error!("clamav write length failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        stream.write_all(data).await.map_err(|err| {
            tracing::error!("clamav write data failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        stream
            .write_all(&0u32.to_be_bytes())
            .await
            .map_err(|err| {
                tracing::error!("clamav write terminator failed: {}", err);
                anyhow::anyhow!("Virus scanner communication error")
            })?;

        stream.flush().await.map_err(|err| {
            tracing::error!("clamav flush failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(|err| {
                tracing::error!("clamav read failed: {}", err);
                anyhow::anyhow!("Virus scanner communication error")
            })?;

        if response.contains("OK") {
            return Ok(true);
        }
        if response.contains("FOUND") {
            tracing::warn!("clamav detected malware: {}", response.trim());
            return Ok(false);
        }

        tracing::error!("clamav unexpected response: {}", response.trim());
        Err(anyhow::anyhow!("Virus scanner returned unexpected response"))
    }
}
