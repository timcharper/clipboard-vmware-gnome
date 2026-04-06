use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const CMD_PUSH: u32 = 1;
pub const CMD_PULL: u32 = 2;

pub async fn write_frame<W: AsyncWriteExt + Unpin>(w: &mut W, mime: &str, data: &[u8]) -> Result<()> {
    let mime_b = mime.as_bytes();
    w.write_u32_le(mime_b.len() as u32).await?;
    w.write_all(mime_b).await?;
    w.write_u32_le(data.len() as u32).await?;
    w.write_all(data).await?;
    Ok(())
}

pub async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> Result<(String, Vec<u8>)> {
    let mime_len = r.read_u32_le().await? as usize;
    let mut mime_b = vec![0u8; mime_len];
    r.read_exact(&mut mime_b).await?;
    let mime = String::from_utf8(mime_b)?;

    let data_len = r.read_u32_le().await? as usize;
    let mut data = vec![0u8; data_len];
    r.read_exact(&mut data).await?;

    Ok((mime, data))
}
