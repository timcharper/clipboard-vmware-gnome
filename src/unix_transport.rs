use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol;

/// Client side of `serve-unix`. The socket at `socket_path` is expected to
/// already be a live local path — either the real daemon socket on this
/// host, or (for the DeskFlow pairing) the local end of a persistent
/// `ssh -L` tunnel forwarding the peer's socket here. Either way, this code
/// has no idea SSH is involved; that's entirely the tunnel unit's job.
pub async fn push(socket_path: &Path, mime: &str, data: &[u8]) -> Result<()> {
    let mut stream = connect(socket_path).await?;
    stream.write_u32_le(protocol::CMD_PUSH).await?;
    protocol::write_frame(&mut stream, mime, data).await?;
    stream.flush().await?;
    println!("Pushed {mime} ({} bytes) to {}.", data.len(), socket_path.display());
    Ok(())
}

pub async fn pull(socket_path: &Path) -> Result<(String, Vec<u8>)> {
    let mut stream = connect(socket_path).await?;
    stream.write_u32_le(protocol::CMD_PULL).await?;
    stream.flush().await?;
    let _status = stream.read_u32_le().await?;
    protocol::read_frame(&mut stream).await
}

async fn connect(socket_path: &Path) -> Result<UnixStream> {
    UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("Failed to connect to {}", socket_path.display()))
}
