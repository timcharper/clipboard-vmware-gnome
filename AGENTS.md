# clip-sync — Agent Notes

## Architecture

Two components:
- **`clipboard-guest.ps1`** — PowerShell TCP server running inside the Windows VM (port 9999)
- **`clip-sync`** — Rust binary on the Linux host (`src/`)

### Rust crate layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI parsing (clap derive), subcommand dispatch |
| `src/clipboard.rs` | Linux clipboard via `wl-paste` / `wl-copy` + SHA256 hashing |
| `src/protocol.rs` | TCP wire protocol helpers (push/pull/get-seq framing) |
| `src/commands/push.rs` | `send()` + `run()` — reads Linux clipboard and pushes to Windows |
| `src/commands/pull.rs` | `receive()` + `run()` — pulls from Windows and sets Linux clipboard |
| `src/commands/daemon.rs` | Auto-sync daemon: AT-SPI focus events + window-calls WM class query |
| `src/commands/doctor.rs` | Health checks: clipboard round-trip, VM reachability, window-calls |

## Wire protocol (TCP, little-endian u32)

| Cmd | Send | Receive |
|-----|------|---------|
| 1 Push | `[1][mimeLen][mimeBytes][dataLen][data]` | — |
| 2 Pull | `[2]` | `[status][mimeLen][mimeBytes][dataLen][data]` |
| 3 GetSeq | `[3]` | `[seq_u32]` |

## Daemon focus detection — key insight

VMware Workstation is an XWayland app. It **does not** emit `StateChanged::Focused enabled=true` when it gains focus. The only signal is the outgoing GNOME-native app emitting `StateChanged::Focused enabled=false`.

The daemon therefore listens for **any** `StateChanged::Focused` event (enabled or not), then immediately queries the window-calls extension for the current focused WM class. This correctly handles both directions:
- Native app → VMware: outgoing app fires `Focused enabled=false`
- VMware → native app: incoming app fires `Focused enabled=true`

## window-calls extension

- DBus destination: `org.gnome.Shell` (NOT its own bus name)
- Object path: `/org/gnome/Shell/Extensions/Windows`
- Interface: `org.gnome.Shell.Extensions.Windows`
- Method: `List()` → JSON array; focused window has `"focus": true` (not `"has_focus"`)

## Change detection strategy

- **Linux → Windows (push on focus-in):** SHA256 hash of clipboard bytes. Hash is taken at daemon start and updated after each sync. Push if hash changed.
- **Windows → Linux (pull on focus-out):** `GetClipboardSequenceNumber()` Win32 API. Baseline recorded after each push (or on focus-in if no push was needed). Pull if sequence number changed (`!=`, not `>`, to handle wraparound).
- Both values are `Option<_>`: `None → Some` is always treated as dirty.

## Environment variables

| Var | Default | Purpose |
|-----|---------|---------|
| `CLIP_SYNC_IP` | `172.16.34.128` | VM IP address |
| `CLIP_SYNC_VM_CLASS` | `Vmplayer` | WM class of the VMware window |
| `RUST_LOG` | `clip_sync=info` | Log filter (uses tracing-subscriber) |
