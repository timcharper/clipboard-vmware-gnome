use anyhow::Result;
use std::path::Path;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixListener;

use crate::protocol;

/// Linux peer-to-peer counterpart to `clipboard-guest.ps1`: a long-running
/// daemon listening on a Unix domain socket, meant to run as a
/// `systemd --user` unit (see systemd/clip-sync.service) so it inherits the
/// graphical session's WAYLAND_DISPLAY/DBUS_SESSION_BUS_ADDRESS natively —
/// no manual env wiring needed.
///
/// Reached from the peer host over a persistent `ssh -L` tunnel forwarding
/// this socket to a local path there (see systemd/clip-sync-tunnel.service),
/// piggybacking on ~/.ssh key auth/encryption rather than exposing this
/// socket over the network directly.
///
/// Each connection carries exactly one request, matching the existing
/// protocol convention (see README "Protocol" table).
pub async fn run_unix(socket_path: &Path) -> Result<()> {
    // Clean up a stale socket file from an unclean previous shutdown; bind
    // fails with AddrInUse otherwise.
    let _ = std::fs::remove_file(socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    println!("clip-sync serve-unix listening on {}", socket_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        // Spawned, not awaited inline: clipboard.rs's subprocess calls are
        // timeout-bounded now, but a slow one (or a burst of concurrent
        // push+pull) should never be able to stall *other* connections
        // behind it. A single sequential accept loop already did exactly
        // that once — a hung `wl-paste` wedged every connection after it
        // for two hours, invisibly (the daemon still looked "active
        // (running)" to systemd the whole time).
        tokio::spawn(async move {
            let (mut reader, mut writer) = stream.into_split();
            if let Err(e) = handle_one(&mut reader, &mut writer).await {
                eprintln!("connection failed: {e:#}");
            }
        });
    }
}

async fn handle_one<R, W>(reader: &mut R, writer: &mut W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let cmd = reader.read_u32_le().await?;
    match cmd {
        protocol::CMD_PUSH => {
            let (mime, data) = protocol::read_frame(reader).await?;
            let len = data.len();
            crate::clipboard::set(&mime, &data).await?;
            println!("received push: {mime} ({len} bytes)");
        }
        protocol::CMD_PULL => {
            let (mime, data) = crate::clipboard::get().await?;
            writer.write_u32_le(0).await?; // status: ok
            protocol::write_frame(writer, &mime, &data).await?;
            writer.flush().await?;
            println!("served pull: {mime} ({} bytes)", data.len());
        }
        protocol::CMD_GET_SEQ => {
            // Linux has no equivalent of Win32's GetClipboardSequenceNumber,
            // so we derive a content fingerprint instead: same content is
            // guaranteed to produce the same "sequence", different content
            // is very likely to produce a different one. That's sufficient
            // for change detection even though it isn't a true monotonic
            // counter.
            let (_, data) = crate::clipboard::get().await.unwrap_or_default();
            let hash = crate::clipboard::hash(&data);
            let seq = u32::from_le_bytes(hash[0..4].try_into().unwrap());
            writer.write_u32_le(seq).await?;
            writer.flush().await?;
        }
        other => {
            eprintln!("unknown command: {other}");
        }
    }
    Ok(())
}
