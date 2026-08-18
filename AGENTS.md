# clip-sync — Agent Notes

## Architecture

Two pairings share the same wire protocol and most of the Rust crate:

- **Windows VM guest:** `clipboard-guest.ps1` (PowerShell TCP server, port
  9999, inside the VM) talks to `clip-sync push`/`pull`/`daemon`/`doctor`
  (raw TCP, `CLIP_SYNC_IP`).
- **Linux peer (e.g. DeskFlow pairing):** `clip-sync serve-unix` (this repo,
  the other host) talks to `clip-sync push`/`pull` over a Unix socket
  (`CLIP_SYNC_SOCKET`) reached through a persistent `ssh -L` tunnel — see
  README "Linux-to-Linux mode". A small watcher script drives push/pull off
  DeskFlow's own log instead of the VM daemon's AT-SPI focus detection (see
  README for the shape of it). That script and the `systemd --user` units
  that keep the tunnel/daemon alive are deployment-specific (hostnames,
  socket paths), so they're NOT in this repo.

### Rust crate layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI parsing (clap derive), subcommand dispatch, transport selection (`CLIP_SYNC_SOCKET` → Unix, else TCP) |
| `src/clipboard.rs` | Linux clipboard via `wl-paste` / `wl-copy` + SHA256 hashing |
| `src/protocol.rs` | Wire protocol framing (push/pull/get-seq) — generic over any `AsyncRead`/`AsyncWrite`, shared by both transports |
| `src/unix_transport.rs` | Client-side push/pull over a Unix socket (Linux peer mode) |
| `src/commands/push.rs` | `send()` + `run()` — TCP client (VM mode) |
| `src/commands/pull.rs` | `receive()` + `run()` — TCP client (VM mode) |
| `src/commands/serve.rs` | `run_unix()` — the Linux-peer daemon (`serve-unix`); one connection = one request, same as the VM's PowerShell server |
| `src/commands/daemon.rs` | VM-only: AT-SPI focus events + window-calls WM class query |
| `src/commands/doctor.rs` | VM-only: health checks (clipboard round-trip, VM reachability, window-calls) |

The DeskFlow-log watcher (tails deskflow-core's log, calls `push`/`pull` on
screen switch) isn't part of this repo — see above.

## Wire protocol (little-endian u32; same framing over TCP or a Unix socket)

| Cmd | Send | Receive |
|-----|------|---------|
| 1 Push | `[1][mimeLen][mimeBytes][dataLen][data]` | — |
| 2 Pull | `[2]` | `[status][mimeLen][mimeBytes][dataLen][data]` |
| 3 GetSeq | `[3]` | `[seq_u32]` |

## Linux-peer mode — key decisions

- **No open port.** An early version bound `serve` to `0.0.0.0` over raw TCP;
  that would let anyone on the LAN read/overwrite the clipboard with zero
  auth. `serve-unix` only listens on a local Unix socket, reached from the
  peer through a persistent `ssh -L` tunnel — piggybacking on `~/.ssh` key
  auth instead of building a new auth mechanism.
- **Persistent tunnel, not per-call SSH exec.** An earlier version spawned
  `ssh <peer> -- clip-sync serve-stdio` fresh for every push/pull, which
  needed a runtime hack to import `WAYLAND_DISPLAY` etc. from
  `systemctl --user show-environment` (a bare `ssh host cmd` doesn't inherit
  the graphical session's env). Moving to a long-running `systemd --user`
  daemon (`clip-sync.service`) sidesteps that entirely — systemd already
  exports the real session env into `--user` units natively. The
  `clip-sync-tunnel.service` unit keeps the `ssh -L` forward alive
  (`ServerAlive*` + `ExitOnForwardFailure` + `Restart=always`), so the
  client side (`unix_transport.rs`) has no idea SSH is involved at all —
  it just connects to a local socket path.
- **No D-Bus hook for DeskFlow screen switches.** Checked directly:
  `org.deskflow.deskflow` isn't an activatable bus name (`busctl
  introspect` fails), and DeskFlow's config schema has no generic
  on-enter/on-leave hook (only keystroke/hotkey bindings). The only
  reliable signal is deskflow-core's own `leaving screen`/`entering screen`
  log lines — real protocol-level events, not a heuristic proxy, but only
  ever emitted by the DeskFlow *server* (the machine with the physical
  mouse). The client side never logs an equivalent of its own, so the
  watcher only runs on the server, driving both directions (push on leave,
  pull on enter) from its single log.

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
