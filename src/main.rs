use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::runtime::Runtime;

mod clipboard;
mod commands;
mod protocol;
mod unix_transport;

const DEFAULT_IP: &str = "172.16.34.128";
const DEFAULT_PORT: u16 = 9999;

fn default_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join("clip-sync.sock")
}

#[derive(Parser)]
#[command(name = "clip-sync")]
#[command(about = "Bidirectional clipboard sync: Linux host <-> Windows VMware guest, or Linux <-> Linux")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Push the local clipboard to the peer (Windows guest, or a Linux peer
    /// if CLIP_SYNC_SOCKET is set)
    Push,
    /// Pull the peer's clipboard into the local clipboard (Windows guest, or
    /// a Linux peer if CLIP_SYNC_SOCKET is set)
    Pull,
    /// Watch for focus changes and sync automatically (Windows VM guest only)
    Daemon {
        /// WM class of the VMware window to watch
        #[arg(long, env = "CLIP_SYNC_VM_CLASS", default_value = "Vmplayer")]
        vm_class: String,
    },
    /// Check that all components are working correctly (Windows VM guest only)
    Doctor,
    /// Run the Linux-peer clipboard daemon on a Unix domain socket. Meant to
    /// run as a systemd --user unit (see systemd/clip-sync.service) so it
    /// inherits the graphical session's environment natively. Reached from
    /// the peer over a persistent `ssh -L` tunnel (see
    /// systemd/clip-sync-tunnel.service), not directly over the network.
    ServeUnix {
        #[arg(long, default_value_os_t = default_socket_path())]
        socket: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("clip_sync=info")),
        )
        .init();

    let args = Cli::parse();
    let ip = std::env::var("CLIP_SYNC_IP").unwrap_or_else(|_| DEFAULT_IP.to_string());
    let rt = Runtime::new()?;

    // CLIP_SYNC_SOCKET selects the Unix-socket transport (a local path,
    // typically the local end of a persistent ssh -L tunnel to a peer's
    // `serve-unix` daemon) instead of the raw-TCP transport used for the
    // Windows VM guest, which can't run clip-sync itself.
    let socket = std::env::var_os("CLIP_SYNC_SOCKET").map(PathBuf::from);

    match args.command {
        Commands::Push => match &socket {
            Some(path) => rt.block_on(async {
                let (mime, data) = clipboard::get().await?;
                unix_transport::push(path, &mime, &data).await
            })?,
            None => rt.block_on(commands::push::run(&ip, DEFAULT_PORT))?,
        },
        Commands::Pull => match &socket {
            Some(path) => rt.block_on(async {
                let (mime, data) = unix_transport::pull(path).await?;
                let len = data.len();
                clipboard::set(&mime, &data).await?;
                println!("Pulled {mime} ({len} bytes) from {}.", path.display());
                anyhow::Ok(())
            })?,
            None => rt.block_on(commands::pull::run(&ip, DEFAULT_PORT))?,
        },
        Commands::Daemon { vm_class } => {
            rt.block_on(commands::daemon::run(ip, DEFAULT_PORT, vm_class))?
        }
        Commands::Doctor => rt.block_on(commands::doctor::run(&ip, DEFAULT_PORT))?,
        Commands::ServeUnix { socket } => rt.block_on(commands::serve::run_unix(&socket))?,
    }

    Ok(())
}
