# clip-sync

Bidirectional clipboard sync between a Linux (Wayland) host and either a
Windows VMware guest (clipboard integration doesn't work with Wayland there)
or a second Linux/Wayland host (e.g. paired over DeskFlow, whose own
Wayland clipboard support is unreliable — see "Linux-to-Linux mode" below).

## Requirements

**Linux host:**
- Rust (to build)
- `wl-clipboard` (`sudo dnf install wl-clipboard`)
- [window-calls GNOME extension](https://github.com/ickyicky/window-calls) (for `daemon` mode)
- AT-SPI accessibility enabled (for `daemon` mode)

**Windows guest:**
- PowerShell 5+

## Setup

Build the host binary:

```bash
cargo build --release
```

Set the VM's IP address (defaults to `172.16.34.128`):

```bash
export CLIP_SYNC_IP=172.16.34.128
```

**Start the receiver on Windows (run once in PowerShell):**

```powershell
.\clipboard-guest.ps1
```

## Usage

```bash
# Push Linux clipboard → Windows
clip-sync push

# Pull Windows clipboard → Linux
clip-sync pull

# Auto-sync on focus change (requires window-calls extension)
clip-sync daemon

# Check that all components are working
clip-sync doctor
```

The `daemon` command watches for focus changes using AT-SPI events. When you
focus the VMware window it pushes the Linux clipboard if it changed; when you
leave the VMware window it pulls the Windows clipboard if it changed.

The VM window is identified by WM class `Vmplayer` by default. Override with:

```bash
export CLIP_SYNC_VM_CLASS=Vmplayer   # default
clip-sync daemon
```

## Linux-to-Linux mode

For two Linux/Wayland hosts (instead of a Windows VM guest) — built for
pairing with [DeskFlow](https://github.com/deskflow/deskflow), whose own
Wayland clipboard sync is unreliable
([deskflow#7600](https://github.com/deskflow/deskflow/issues/7600),
[#8031](https://github.com/deskflow/deskflow/issues/8031),
[#8165](https://github.com/deskflow/deskflow/issues/8165)).

Deliberately **not** an open TCP port: `serve-unix` only listens on a local
Unix domain socket, reached from the peer over a persistent `ssh -L` tunnel.
That piggybacks on your existing `~/.ssh` key auth/`authorized_keys` and
encryption instead of exposing an unauthenticated port that would let
anyone on the LAN read or overwrite the clipboard.

Recommended: wrap both ends in `systemd --user` units so the tunnel
auto-reconnects and the daemon survives reboots — one running
`clip-sync serve-unix` (bind `$XDG_RUNTIME_DIR/clip-sync.sock`) on the
machine being pushed/pulled to, one running a persistent `ssh -N -L`
forward (`ServerAlive*`/`ExitOnForwardFailure` + `Restart=always`) on the
machine doing the pushing/pulling. Not included here — the unit files are
deployment-specific (hostnames, socket paths). Once both are up:

```bash
export CLIP_SYNC_SOCKET=~/.local/state/clip-sync-peer.sock
clip-sync push
clip-sync pull
```

`serve-unix` implements the same wire protocol as `clipboard-guest.ps1`
(push, pull, get-seq — including non-text MIME types, images included), so
`push`/`pull` work unmodified against it once `CLIP_SYNC_SOCKET` is set.

There's no `daemon` mode for this pairing — DeskFlow doesn't expose any
D-Bus signal, IPC socket, or config-level hook for "cursor just switched
hosts" (checked: `org.deskflow.deskflow` isn't an activatable bus name, and
its config schema has no generic on-enter/on-leave action, unlike its
keystroke/hotkey bindings). Instead, tail deskflow-core's log file and sync
on the real protocol-level switch events — not a heuristic proxy:

```bash
# deskflow-core must be logging to a real file, e.g.:
deskflow-core server >~/.local/state/deskflow-core.log 2>&1 &

export CLIP_SYNC_SOCKET=~/.local/state/clip-sync-peer.sock
tail -n0 -F ~/.local/state/deskflow-core.log | while IFS= read -r line; do
  case "$line" in
    *"leaving screen"*)  clip-sync push ;;
    *"entering screen"*) clip-sync pull ;;
  esac
done
```

Only the DeskFlow *server* (the machine with the physical mouse) ever logs
`leaving screen`/`entering screen` — the client never sees an equivalent
line of its own. So run the watcher above only there, driving both
directions from its single log: `leaving screen` pushes the local clipboard
to the peer, `entering screen` pulls the peer's clipboard back. The peer
just needs `serve-unix` running — no watcher, no log file, nothing
client-side beyond the daemon.

## How it works

### Change detection

- **Linux side:** SHA256 hash of clipboard content, taken at daemon start and
  updated after each sync. If the hash differs on focus-in, the clipboard is
  pushed to Windows.
- **Windows side:** `GetClipboardSequenceNumber()` (Win32 API) recorded after
  each push. If the sequence number changed when focus leaves the VM, the
  clipboard is pulled to Linux.

### Focus detection

AT-SPI `StateChanged::Focused` events trigger a query to the window-calls
GNOME extension (`org.gnome.Shell` / `/org/gnome/Shell/Extensions/Windows`),
which returns the currently focused window's WM class. A 150ms debounce
absorbs burst events before the query fires.

**Important:** VMware Workstation is an XWayland app and does not emit
`Focused enabled=true` when it gains focus. Instead, we detect the transition
by watching for `Focused` state changes on the *outgoing* window, then
querying the WM class of whatever is focused at that moment.

## Protocol

Each connection is a single request over TCP on port 9999.

| Command | Request | Response |
|---------|---------|----------|
| Push (1) | `[cmd(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]` | none |
| Pull (2) | `[cmd(4)]` | `[status(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]` |
| GetSeq (3) | `[cmd(4)]` | `[seq(4)]` |

All integers are little-endian. Supported MIME types: `text/plain`, `image/png`,
`image/jpeg`, and other image formats.
