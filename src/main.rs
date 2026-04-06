use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;

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
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let ip = std::env::var("CLIP_SYNC_IP").unwrap_or_else(|_| DEFAULT_IP.to_string());
    let rt = Runtime::new()?;

    match args.command {
        Commands::Push => rt.block_on(commands::push::run(&ip, DEFAULT_PORT))?,
        Commands::Pull => rt.block_on(commands::pull::run(&ip, DEFAULT_PORT))?,
    }

    Ok(())
}
