use anyhow::Result;
use atspi::AccessibilityConnection;
use atspi::connection::common::events::{object::StateChangedEvent, Event, ObjectEvents};
use futures_lite::StreamExt;
use std::time::Duration;

use crate::{clipboard, protocol};
use crate::commands::{push, pull};

#[zbus::proxy(
    interface = "org.gnome.Shell.Extensions.Windows",
    default_service = "org.gnome.Shell",
    default_path = "/org/gnome/Shell/Extensions/Windows"
)]
pub trait WindowCalls {
    async fn list(&self) -> zbus::Result<String>;
}

#[derive(serde::Deserialize, Debug)]
struct WindowInfo {
    wm_class: Option<String>,
    #[serde(default)]
    focus: bool,
}

struct State {
    /// SHA256 of the last Linux clipboard content we pushed or pulled.
    /// None means we haven't snapshotted yet → treat as dirty on next focus-in.
    linux_hash: Option<[u8; 32]>,
    /// Windows clipboard sequence number recorded after our last push or on focus-in.
    /// None means VM was unreachable → treat as dirty on next focus-out.
    windows_seq: Option<u32>,
    vm_focused: bool,
}

async fn get_focused_wm_class(proxy: &WindowCallsProxy<'_>) -> Option<String> {
    let json = proxy.list().await.ok()?;
    let windows: Vec<WindowInfo> = serde_json::from_str(&json).ok()?;
    windows.into_iter().find(|w| w.focus)?.wm_class
}

async fn on_focus_in(state: &mut State, ip: &str, port: u16) {
    eprintln!("[clip-sync] VMware focused");

    match clipboard::get().await {
        Ok((mime, data)) => {
            let hash = clipboard::hash(&data);
            let dirty = state.linux_hash.map_or(true, |h| h != hash);
            if dirty {
                eprintln!("[clip-sync] Linux clipboard changed — pushing to Windows");
                match push::send(ip, port, &mime, &data).await {
                    Ok(()) => state.linux_hash = Some(hash),
                    Err(e) => eprintln!("[clip-sync] Push failed: {e}"),
                }
            } else {
                eprintln!("[clip-sync] Linux clipboard unchanged — skipping push");
            }
        }
        Err(e) => eprintln!("[clip-sync] Could not read Linux clipboard: {e}"),
    }

    // Always establish a fresh sequence baseline, even if we didn't push.
    state.windows_seq = protocol::get_sequence_number(ip, port).await;
    match state.windows_seq {
        Some(seq) => eprintln!("[clip-sync] Windows seq baseline: {seq}"),
        None => eprintln!("[clip-sync] Could not reach Windows guest (script running?)"),
    }
}

async fn on_focus_out(state: &mut State, ip: &str, port: u16) {
    eprintln!("[clip-sync] VMware blurred");

    let current_seq = protocol::get_sequence_number(ip, port).await;
    let should_pull = match (state.windows_seq, current_seq) {
        (prev, Some(curr)) => prev.map_or(true, |p| curr != p),
        _ => false,
    };

    if should_pull {
        eprintln!("[clip-sync] Windows clipboard changed — pulling to Linux");
        match pull::receive(ip, port).await {
            Ok((mime, data)) => match clipboard::set(&mime, &data).await {
                Ok(()) => {
                    eprintln!("[clip-sync] Pulled {mime} ({} bytes)", data.len());
                    state.linux_hash = Some(clipboard::hash(&data));
                }
                Err(e) => eprintln!("[clip-sync] Failed to set Linux clipboard: {e}"),
            },
            Err(e) => eprintln!("[clip-sync] Pull failed: {e}"),
        }
    } else {
        eprintln!("[clip-sync] Windows clipboard unchanged — skipping pull");
    }
}

pub async fn run(ip: String, port: u16, vm_class: String) -> Result<()> {
    let atspi = AccessibilityConnection::new().await?;
    atspi.register_event::<StateChangedEvent>().await?;

    let session = zbus::Connection::session().await?;
    let windows = WindowCallsProxy::new(&session).await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Spawn AT-SPI listener
    let atspi2 = atspi.clone();
    tokio::spawn(async move {
        let mut stream = atspi2.event_stream();
        while let Some(result) = stream.next().await {
            if let Ok(Event::Object(ObjectEvents::StateChanged(ev))) = result {
                if ev.state == atspi_common::State::Focused {
                    let _ = tx.send(());
                }
            }
        }
    });

    // Take initial Linux clipboard snapshot so first focus-in doesn't always push.
    let mut state = State {
        linux_hash: clipboard::get().await.ok().map(|(_, d)| clipboard::hash(&d)),
        windows_seq: None,
        vm_focused: false,
    };

    eprintln!("[clip-sync] Daemon running (vm_class={vm_class}, target={ip}:{port})");

    loop {
        if rx.recv().await.is_none() {
            break;
        }

        // Drain burst with 150ms quiet window
        loop {
            match tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
                Ok(Some(())) => {}
                _ => break,
            }
        }

        let wm = get_focused_wm_class(&windows).await;
        let now_vm = wm.as_deref() == Some(vm_class.as_str());

        if now_vm == state.vm_focused {
            continue;
        }

        if now_vm {
            on_focus_in(&mut state, &ip, port).await;
        } else {
            on_focus_out(&mut state, &ip, port).await;
        }
        state.vm_focused = now_vm;
    }

    Ok(())
}
