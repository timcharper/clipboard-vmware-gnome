use anyhow::{Context, Result, bail};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::process::Command;
use crate::protocol;

const MIME_PRIORITY: &[&str] = &[
    "image/png", "image/jpeg", "image/gif", "image/bmp", "image/webp",
    "text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "STRING",
];

fn get_clipboard() -> Result<(String, Vec<u8>)> {
    let output = Command::new("wl-paste")
        .arg("--list-types")
        .output()
        .context("Failed to run wl-paste --list-types")?;

    let available: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if available.is_empty() {
        bail!("Clipboard is empty or wl-paste failed");
    }

    let mime = MIME_PRIORITY
        .iter()
        .find(|&&p| available.iter().any(|a| a == p))
        .map(|&s| s.to_string())
        .unwrap_or_else(|| available[0].clone());

    let data = Command::new("wl-paste")
        .args(["--no-newline", "--type", &mime])
        .output()
        .context("Failed to run wl-paste")?
        .stdout;

    // Normalize charset variants so Windows sees plain text/plain
    let mime = if mime.starts_with("text/plain") {
        "text/plain".to_string()
    } else {
        mime
    };

    Ok((mime, data))
}

pub async fn run(ip: &str, port: u16) -> Result<()> {
    let (mime, data) = get_clipboard()?;
    let mut stream = TcpStream::connect((ip, port)).await
        .with_context(|| format!("Failed to connect to {ip}:{port}"))?;

    stream.write_u32_le(protocol::CMD_PUSH).await?;
    protocol::write_frame(&mut stream, &mime, &data).await?;
    stream.flush().await?;

    println!("Pushed {} ({} bytes) to Windows.", mime, data.len());
    Ok(())
}
