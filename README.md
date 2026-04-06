# clip-sync

Bidirectional clipboard sync between a Linux (Wayland) host and a Windows VMware guest, since VMware's clipboard integration doesn't work with Wayland.

## Requirements

**Linux host:**
- Rust (to build)
- `wl-clipboard` (`sudo dnf install wl-clipboard`)

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

## Usage

**Start the receiver on Windows (run once in PowerShell):**

```powershell
.\clipboard-guest.ps1
```

**Sync clipboard from Linux host:**

```bash
# Push Linux clipboard → Windows
clip-sync push

# Pull Windows clipboard → Linux
clip-sync pull
```

## Protocol

Each connection is a single request over TCP on port 9999.

**Push (command 1):** `[cmd(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]`

**Pull (command 2):** `[cmd(4)]` → response: `[status(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]`

All integers are little-endian. Supported MIME types: `text/plain`, `image/png`, `image/jpeg`, and other image formats.

## Automation

To sync automatically on focus change, see the GNOME extension approach: watch `global.display.notify::focus-window`, check the WM class, and run `push` or `pull` accordingly.
