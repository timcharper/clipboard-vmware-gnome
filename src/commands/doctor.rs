use crate::{clipboard, protocol};
use crate::commands::{push, pull};

struct Check {
    label: String,
    status: Status,
    detail: Option<String>,
}

enum Status {
    Ok,
    Warn,
    Fail,
}

impl Check {
    fn ok(label: impl Into<String>) -> Self {
        Self { label: label.into(), status: Status::Ok, detail: None }
    }
    fn ok_detail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { label: label.into(), status: Status::Ok, detail: Some(detail.into()) }
    }
    fn warn(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { label: label.into(), status: Status::Warn, detail: Some(detail.into()) }
    }
    fn fail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { label: label.into(), status: Status::Fail, detail: Some(detail.into()) }
    }
    fn print(&self) {
        let (icon, color) = match self.status {
            Status::Ok   => ("✓", "\x1b[32m"),
            Status::Warn => ("!", "\x1b[33m"),
            Status::Fail => ("✗", "\x1b[31m"),
        };
        let reset = "\x1b[0m";
        let dim = "\x1b[2m";
        print!("  {color}{icon}{reset} {}", self.label);
        if let Some(d) = &self.detail {
            print!("  {dim}{d}{reset}");
        }
        println!();
    }
}

pub async fn run(ip: &str, port: u16) -> anyhow::Result<()> {
    let mut any_fail = false;

    // ── Wayland Clipboard ────────────────────────────────────────────────────

    println!("\n\x1b[1mWayland Clipboard\x1b[0m");

    let test_str = b"clip-sync-doctor-test";
    let clipboard_ok = match clipboard::set("text/plain", test_str) {
        Err(e) => {
            any_fail = true;
            Check::fail("wl-copy (write)", format!("{e}")).print();
            false
        }
        Ok(()) => {
            Check::ok("wl-copy (write)").print();
            match clipboard::get() {
                Err(e) => {
                    any_fail = true;
                    Check::fail("wl-paste (read)", format!("{e}")).print();
                    false
                }
                Ok((mime, data)) => {
                    if data == test_str {
                        Check::ok_detail("wl-paste (read)", format!("round-trip OK  mime={mime}")).print();
                        true
                    } else {
                        any_fail = true;
                        Check::fail(
                            "wl-paste (read)",
                            format!("round-trip mismatch — got {} bytes of {mime}", data.len()),
                        ).print();
                        false
                    }
                }
            }
        }
    };

    // ── Windows Guest ────────────────────────────────────────────────────────

    println!("\n\x1b[1mWindows Guest ({ip}:{port})\x1b[0m");

    let seq_before = protocol::get_sequence_number(ip, port).await;
    match seq_before {
        None => {
            any_fail = true;
            Check::fail(
                "Reachable",
                format!("could not connect to {ip}:{port} — is clipboard-guest.ps1 running?"),
            ).print();
        }
        Some(seq) => {
            Check::ok_detail("Reachable", format!("clipboard sequence number: {seq}")).print();

            // Push test string to Windows
            if clipboard_ok {
                match push::send(ip, port, "text/plain", test_str).await {
                    Err(e) => {
                        any_fail = true;
                        Check::fail("Push to Windows", format!("{e}")).print();
                    }
                    Ok(()) => {
                        Check::ok("Push to Windows").print();

                        // Confirm sequence number incremented
                        match protocol::get_sequence_number(ip, port).await {
                            None => {
                                any_fail = true;
                                Check::fail("Sequence number after push", "lost connection").print();
                            }
                            Some(seq_after) if seq_after != seq => {
                                Check::ok_detail(
                                    "Sequence number after push",
                                    format!("{seq} → {seq_after}"),
                                ).print();

                                // Pull back and verify it matches
                                match pull::receive(ip, port).await {
                                    Err(e) => {
                                        any_fail = true;
                                        Check::fail("Pull from Windows", format!("{e}")).print();
                                    }
                                    Ok((mime, data)) => {
                                        if data == test_str {
                                            Check::ok_detail(
                                                "Pull from Windows",
                                                format!("round-trip OK  mime={mime}"),
                                            ).print();
                                        } else {
                                            any_fail = true;
                                            Check::fail(
                                                "Pull from Windows",
                                                format!(
                                                    "round-trip mismatch — sent {:?}, got {:?}",
                                                    String::from_utf8_lossy(test_str),
                                                    String::from_utf8_lossy(&data),
                                                ),
                                            ).print();
                                        }
                                    }
                                }
                            }
                            Some(seq_after) => {
                                any_fail = true;
                                Check::fail(
                                    "Sequence number after push",
                                    format!("did not increment ({seq} → {seq_after})"),
                                ).print();
                            }
                        }
                    }
                }
            }
        }
    }

    // ── window-calls Extension ───────────────────────────────────────────────

    println!("\n\x1b[1mwindow-calls Extension\x1b[0m");

    match zbus::Connection::session().await {
        Err(e) => {
            any_fail = true;
            Check::fail("D-Bus session bus", format!("could not connect: {e}")).print();
        }
        Ok(conn) => {
            Check::ok("D-Bus session bus").print();

            match super::daemon::WindowCallsProxy::new(&conn).await {
                Err(e) => {
                    any_fail = true;
                    Check::fail("window-calls extension", format!("could not create proxy: {e}")).print();
                }
                Ok(proxy) => match proxy.list().await {
                            Err(e) => {
                                any_fail = true;
                                Check::fail("List() call", format!("{e}")).print();
                            }
                            Ok(json) => {
                                let windows: Vec<serde_json::Value> =
                                    serde_json::from_str(&json).unwrap_or_default();
                                let focused = windows.iter()
                                    .find(|w| w["focus"].as_bool().unwrap_or(false));
                                match focused {
                                    Some(w) => {
                                        let wm = w["wm_class"].as_str().unwrap_or("?");
                                        Check::ok_detail(
                                            "List() call",
                                            format!("{} windows, focused wm_class={wm}", windows.len()),
                                        ).print();
                                    }
                                    None => Check::warn(
                                        "List() call",
                                        format!("{} windows, none marked focus", windows.len()),
                                    ).print(),
                                }
                            }
                        },
            }
        }
    }

    // ── Summary ──────────────────────────────────────────────────────────────

    println!();
    if any_fail {
        println!("\x1b[31mSome checks failed.\x1b[0m Fix the issues above and re-run `clip-sync doctor`.");
        std::process::exit(1);
    } else {
        println!("\x1b[32mAll checks passed.\x1b[0m");
    }

    Ok(())
}
