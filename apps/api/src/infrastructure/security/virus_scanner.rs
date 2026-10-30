use anyhow::Result;
use bytes::Bytes;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct VirusScanner {
    enabled: bool,
    host: String,
    port: u16,
    timeout: Duration,
}

impl VirusScanner {
    pub fn new(enabled: bool, host: Option<String>, port: Option<u16>, timeout: Duration) -> Self {
        Self {
            enabled,
            host: host.unwrap_or_else(|| "clamav".to_string()),
            port: port.unwrap_or(3310),
            timeout,
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
        tokio::time::timeout(self.timeout, self.scan_inner(data))
            .await
            .map_err(|_| anyhow::anyhow!("Virus scanner deadline exceeded"))?
    }

    async fn scan_inner(&self, data: &Bytes) -> Result<bool> {
        let mut stream = TcpStream::connect(format!("{}:{}", self.host, self.port))
            .await
            .map_err(|err| {
                tracing::error!("clamav unavailable (fail-closed): {}", err);
                anyhow::anyhow!("Virus scanner unavailable — upload rejected for safety")
            })?;

        stream.write_all(b"zINSTREAM\0").await.map_err(|err| {
            tracing::error!("clamav write failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        let len = u32::try_from(data.len())?;
        stream.write_all(&len.to_be_bytes()).await.map_err(|err| {
            tracing::error!("clamav write length failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        stream.write_all(data).await.map_err(|err| {
            tracing::error!("clamav write data failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        stream.write_all(&0u32.to_be_bytes()).await.map_err(|err| {
            tracing::error!("clamav write terminator failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        stream.flush().await.map_err(|err| {
            tracing::error!("clamav flush failed: {}", err);
            anyhow::anyhow!("Virus scanner communication error")
        })?;

        // zINSTREAM uses a NUL terminator; the peer can keep its connection open.
        let mut response = Vec::new();
        BufReader::new(stream.take(1025))
            .read_until(0, &mut response)
            .await
            .map_err(|err| {
                tracing::error!("clamav read failed: {}", err);
                anyhow::anyhow!("Virus scanner communication error")
            })?;

        anyhow::ensure!(
            response.len() <= 1024 && response.last() == Some(&0),
            "Invalid virus scanner reply"
        );
        let response = std::str::from_utf8(&response[..response.len() - 1])?;
        if response == "stream: OK" {
            return Ok(true);
        }
        if response.starts_with("stream: ") && response.ends_with(" FOUND") && response.len() > 14 {
            tracing::warn!("ClamAV detected malware");
            return Ok(false);
        }

        tracing::error!("ClamAV returned an unexpected response");
        Err(anyhow::anyhow!(
            "Virus scanner returned unexpected response"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    async fn scanner(reply: Vec<u8>, keep_open: bool) -> (VirusScanner, Server) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = Server(tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut command = [0; 10];
            stream.read_exact(&mut command).await.unwrap();
            assert_eq!(&command, b"zINSTREAM\0");
            let length = stream.read_u32().await.unwrap();
            assert_eq!(length, 4);
            let mut body = [0; 4];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"file");
            assert_eq!(stream.read_u32().await.unwrap(), 0);
            stream.write_all(&reply).await.unwrap();
            if keep_open {
                std::future::pending::<()>().await;
            }
        }));
        (
            VirusScanner::new(
                true,
                Some("127.0.0.1".into()),
                Some(port),
                Duration::from_secs(2),
            ),
            server,
        )
    }

    #[tokio::test]
    async fn terminated_clean_reply_does_not_wait_for_connection_close() {
        let (scanner, _server) = scanner(b"stream: OK\0".to_vec(), true).await;
        assert!(scanner.scan(&Bytes::from_static(b"file")).await.unwrap());
    }

    #[tokio::test]
    async fn malware_signature_containing_ok_is_rejected() {
        let (scanner, _server) =
            scanner(b"stream: Win.Test-OK-Signature FOUND\0".to_vec(), true).await;
        assert!(!scanner.scan(&Bytes::from_static(b"file")).await.unwrap());
    }

    #[tokio::test]
    async fn oversized_unterminated_and_unresponsive_replies_fail_closed() {
        for (reply, keep_open) in [
            (vec![b'x'; 1025], true),
            (b"stream: OK".to_vec(), false),
            (b"stream: ERROR\0".to_vec(), true),
            (Vec::new(), true),
        ] {
            let (scanner, _server) = scanner(reply, keep_open).await;
            assert!(scanner.scan(&Bytes::from_static(b"file")).await.is_err());
        }
    }
}
