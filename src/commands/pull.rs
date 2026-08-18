use anyhow::{Context, Result};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use crate::protocol;

pub async fn receive(ip: &str, port: u16) -> Result<(String, Vec<u8>)> {
    let mut stream = TcpStream::connect((ip, port)).await
        .with_context(|| format!("Failed to connect to {ip}:{port}"))?;
    stream.write_u32_le(protocol::CMD_PULL).await?;
    stream.flush().await?;
    let _status = tokio::io::AsyncReadExt::read_u32_le(&mut stream).await?;
    let (mime, data) = protocol::read_frame(&mut stream).await?;
    Ok((mime, data))
}

pub async fn run(ip: &str, port: u16) -> Result<()> {
    let (mime, data) = receive(ip, port).await?;
    crate::clipboard::set(&mime, &data).await?;
    println!("Pulled {mime} ({} bytes) from Windows into clipboard.", data.len());
    Ok(())
}
