use anyhow::{Context, Result};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::process::{Command, Stdio};
use std::io::Write;
use crate::protocol;

pub async fn run(ip: &str, port: u16) -> Result<()> {
    let mut stream = TcpStream::connect((ip, port)).await
        .with_context(|| format!("Failed to connect to {ip}:{port}"))?;

    stream.write_u32_le(protocol::CMD_PULL).await?;
    stream.flush().await?;

    // Response: [status(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]
    let _status = tokio::io::AsyncReadExt::read_u32_le(&mut stream).await?;
    let (mime, data) = protocol::read_frame(&mut stream).await?;

    let mut child = Command::new("wl-copy")
        .args(["--type", &mime])
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to run wl-copy")?;

    child.stdin.take().unwrap().write_all(&data)?;
    child.wait()?;

    println!("Pulled {} ({} bytes) from Windows into clipboard.", mime, data.len());
    Ok(())
}
