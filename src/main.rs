use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;

mod clipboard;
mod commands;
mod protocol;

const DEFAULT_IP: &str = "172.16.34.128";
const DEFAULT_PORT: u16 = 9999;

#[derive(Parser)]
#[command(name = "clip-sync")]
#[command(about = "Bidirectional clipboard sync between Linux host and Windows VMware guest")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Push the Linux clipboard to the Windows guest
    Push,
    /// Pull the Windows guest clipboard into the Linux clipboard
    Pull,
    /// Watch for focus changes and sync automatically
    Daemon {
        /// WM class of the VMware window to watch
        #[arg(long, env = "CLIP_SYNC_VM_CLASS", default_value = "Vmplayer")]
        vm_class: String,
    },
    /// Check that all components are working correctly
    Doctor,
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

    match args.command {
        Commands::Push => rt.block_on(commands::push::run(&ip, DEFAULT_PORT))?,
        Commands::Pull => rt.block_on(commands::pull::run(&ip, DEFAULT_PORT))?,
        Commands::Daemon { vm_class } => {
            rt.block_on(commands::daemon::run(ip, DEFAULT_PORT, vm_class))?
        }
        Commands::Doctor => rt.block_on(commands::doctor::run(&ip, DEFAULT_PORT))?,
    }

    Ok(())
}
