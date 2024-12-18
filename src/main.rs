use anyhow::Result;
use clap::{Parser, Subcommand};
use log::info;

mod app;
mod network;
mod protocol;
mod ui;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a file to another peer
    Send {
        /// Path to the file to send
        #[arg(required = true)]
        file_path: String,
    },
    /// Receive a file from another peer
    Receive {
        /// Directory to save the received file
        #[arg(default_value = ".")]
        output_dir: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Parse command line arguments
    let cli = Cli::parse();

    // Handle commands
    match cli.command {
        Commands::Send { file_path } => {
            info!("Sending file: {}", file_path);
            // TODO: Implement send logic
            Ok(())
        }
        Commands::Receive { output_dir } => {
            info!("Receiving file into directory: {}", output_dir);
            // TODO: Implement receive logic
            Ok(())
        }
    }
}
