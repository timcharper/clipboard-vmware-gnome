use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::{Command, Stdio};

const MIME_PRIORITY: &[&str] = &[
    "image/png", "image/jpeg", "image/gif", "image/bmp", "image/webp",
    "text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "STRING",
];

pub fn get() -> Result<(String, Vec<u8>)> {
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
        bail!("Clipboard is empty or wl-paste returned no types");
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

pub fn set(mime: &str, data: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to run wl-copy")?;
    child.stdin.take().unwrap().write_all(data)?;
    child.wait()?;
    Ok(())
}

pub fn hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}
