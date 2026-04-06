use anyhow::{Context, Result};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use crate::protocol;

pub async fn send(ip: &str, port: u16, mime: &str, data: &[u8]) -> Result<()> {
    let mut stream = TcpStream::connect((ip, port)).await
        .with_context(|| format!("Failed to connect to {ip}:{port}"))?;
    stream.write_u32_le(protocol::CMD_PUSH).await?;
    protocol::write_frame(&mut stream, mime, data).await?;
    stream.flush().await?;
    println!("Pushed {mime} ({} bytes) to Windows.", data.len());
    Ok(())
}

pub async fn run(ip: &str, port: u16) -> Result<()> {
    let (mime, data) = crate::clipboard::get()?;
    send(ip, port, &mime, &data).await
}
