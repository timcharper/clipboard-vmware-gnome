use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

const MIME_PRIORITY: &[&str] = &[
    "image/png", "image/jpeg", "image/gif", "image/bmp", "image/webp",
    "text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "STRING",
];

// wl-paste/wl-copy talk to the Wayland compositor; if it's ever unresponsive
// (screen locked, session in a weird state, whatever) they can hang
// indefinitely with no built-in timeout of their own. That once wedged the
// serve-unix daemon for two hours straight — not just that one request, but
// every connection after it too, since the accept loop was sequential. Fixed
// on both fronts: each connection is now its own spawned task (serve.rs), and
// every clipboard subprocess call here is bounded — on timeout we kill the
// child rather than leaking a hung process and blocking-pool thread forever.
const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(5);

/// On timeout, explicitly kill *and reap* the child — don't rely on
/// kill_on_drop alone. Verified empirically that it isn't enough by itself:
/// dropping a Child mid-`wait_with_output()` sends the kill signal, but
/// nothing then calls waitpid() on it, so it sits as a zombie under the
/// daemon indefinitely (watched one survive 5+ seconds with the daemon
/// otherwise healthy and responsive — not reaped by any background tokio
/// machinery, contrary to what the doc comment here used to claim). So this
/// keeps `child` owned throughout — including the timeout branch — instead
/// of moving it into `wait_with_output()`, specifically so we can still
/// call kill()+wait() on it ourselves when time runs out.
async fn run_bounded(mut cmd: Command) -> Result<std::process::Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to spawn {cmd:?}"))?;

    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");

    let collect = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let (out_res, err_res, status) = tokio::join!(
            stdout_pipe.read_to_end(&mut stdout),
            stderr_pipe.read_to_end(&mut stderr),
            child.wait(),
        );
        out_res.context("reading subprocess stdout")?;
        err_res.context("reading subprocess stderr")?;
        let status = status.context("subprocess wait failed")?;
        anyhow::Ok(std::process::Output { status, stdout, stderr })
    };

    match tokio::time::timeout(SUBPROCESS_TIMEOUT, collect).await {
        Ok(result) => result,
        Err(_) => {
            reap(&mut child).await;
            bail!("subprocess timed out after {SUBPROCESS_TIMEOUT:?}")
        }
    }
}

/// Explicit kill-and-wait, so the process is actually gone (not a zombie)
/// by the time this returns — see run_bounded's doc comment for why
/// kill_on_drop alone isn't sufficient.
async fn reap(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

pub async fn get() -> Result<(String, Vec<u8>)> {
    let mut cmd = Command::new("wl-paste");
    cmd.arg("--list-types");
    let output = run_bounded(cmd).await.context("wl-paste --list-types")?;

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

    let mut cmd = Command::new("wl-paste");
    cmd.args(["--no-newline", "--type", &mime]);
    let data = run_bounded(cmd).await.context("wl-paste")?.stdout;

    // Normalize charset variants so Windows sees plain text/plain
    let mime = if mime.starts_with("text/plain") {
        "text/plain".to_string()
    } else {
        mime
    };

    Ok((mime, data))
}

pub async fn set(mime: &str, data: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to run wl-copy")?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(data).await?;
    drop(stdin);

    match tokio::time::timeout(SUBPROCESS_TIMEOUT, child.wait()).await {
        Ok(status) => {
            status.context("wl-copy I/O failed")?;
        }
        Err(_) => {
            reap(&mut child).await;
            bail!("wl-copy timed out after {SUBPROCESS_TIMEOUT:?}")
        }
    }
    Ok(())
}

pub fn hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}
