use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const CMD_PUSH: u32 = 1;
pub const CMD_PULL: u32 = 2;
pub const CMD_GET_SEQ: u32 = 3;

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

/// Returns None if the VM guest is unreachable or the script isn't running.
pub async fn get_sequence_number(ip: &str, port: u16) -> Option<u32> {
    async {
        let mut stream = TcpStream::connect((ip, port)).await?;
        stream.write_u32_le(CMD_GET_SEQ).await?;
        stream.flush().await?;
        let seq = stream.read_u32_le().await?;
        Ok::<u32, anyhow::Error>(seq)
    }
    .await
    .ok()
}
