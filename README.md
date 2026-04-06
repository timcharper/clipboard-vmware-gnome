# clip-sync

Bidirectional clipboard sync between a Linux (Wayland) host and a Windows VMware guest, since VMware's clipboard integration doesn't work with Wayland.

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
