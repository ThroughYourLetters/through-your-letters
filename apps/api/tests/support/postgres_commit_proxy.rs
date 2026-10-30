//! Local PostgreSQL fault fixture. Drop a successful upload COMMIT response,
//! forwarding authentication and row data without recording their contents.

use std::{
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Default)]
struct Fault {
    armed: AtomicBool,
    upload_seen: AtomicBool,
    dropped: AtomicBool,
}

pub struct CommitDropProxy {
    pub address: SocketAddr,
    fault: Arc<Fault>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for CommitDropProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl CommitDropProxy {
    pub async fn start(host: String, port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fault = Arc::new(Fault::default());
        let task = tokio::spawn({
            let fault = fault.clone();
            async move {
                loop {
                    let Ok((client, _)) = listener.accept().await else {
                        return;
                    };
                    let host = host.clone();
                    let fault = fault.clone();
                    tokio::spawn(async move {
                        let _ = forward(client, &host, port, fault).await;
                    });
                }
            }
        });
        Self {
            address,
            fault,
            task,
        }
    }

    pub fn arm(&self) {
        self.fault.upload_seen.store(false, Ordering::SeqCst);
        self.fault.armed.store(true, Ordering::SeqCst);
    }
    pub fn dropped_commit_acknowledgement(&self) -> bool {
        self.fault.dropped.load(Ordering::SeqCst)
    }
}

async fn forward(
    mut client: TcpStream,
    host: &str,
    port: u16,
    fault: Arc<Fault>,
) -> io::Result<()> {
    let mut server = TcpStream::connect((host, port)).await?;
    loop {
        let length = client.read_u32().await?;
        if !(8..=1_048_576).contains(&length) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut startup = vec![0; length as usize - 4];
        client.read_exact(&mut startup).await?;
        server.write_u32(length).await?;
        server.write_all(&startup).await?;
        // This fixture's local database uses plaintext PostgreSQL. Handle a
        // driver's initial SSL probe, then forward the real startup packet.
        if startup == 80877103_u32.to_be_bytes() {
            let response = server.read_u8().await?;
            client.write_u8(response).await?;
            if response != b'N' {
                return Err(io::ErrorKind::Unsupported.into());
            }
            continue;
        }
        break;
    }
    let (client_read, client_write) = client.into_split();
    let (server_read, server_write) = server.into_split();
    tokio::select! {
        result=frontend(client_read,server_write,fault.clone()) => result,
        result=backend(server_read,client_write,fault) => result,
    }
}

async fn message(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<(u8, Vec<u8>)> {
    let kind = reader.read_u8().await?;
    let length = reader.read_u32().await?;
    if !(4..=16_777_216).contains(&length) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut payload = vec![0; length as usize - 4];
    reader.read_exact(&mut payload).await?;
    Ok((kind, payload))
}

async fn write(writer: &mut (impl AsyncWrite + Unpin), kind: u8, payload: &[u8]) -> io::Result<()> {
    writer.write_u8(kind).await?;
    writer.write_u32(payload.len() as u32 + 4).await?;
    writer.write_all(payload).await
}

async fn frontend(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
    fault: Arc<Fault>,
) -> io::Result<()> {
    loop {
        let (kind, payload) = message(&mut reader).await?;
        if matches!(kind, b'P' | b'Q')
            && fault.armed.load(Ordering::SeqCst)
            && payload
                .windows(b"INSERT INTO letterings".len())
                .any(|bytes| bytes == b"INSERT INTO letterings")
        {
            fault.upload_seen.store(true, Ordering::SeqCst);
        }
        write(&mut writer, kind, &payload).await?;
    }
}

async fn backend(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
    fault: Arc<Fault>,
) -> io::Result<()> {
    loop {
        let (kind, payload) = message(&mut reader).await?;
        if kind == b'C'
            && payload == b"COMMIT\0"
            && fault.upload_seen.load(Ordering::SeqCst)
            && fault.armed.swap(false, Ordering::SeqCst)
        {
            fault.dropped.store(true, Ordering::SeqCst);
            return Ok(());
        }
        write(&mut writer, kind, &payload).await?;
    }
}
