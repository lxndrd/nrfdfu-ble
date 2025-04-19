mod package;
mod protocol;
mod transport;
// TODO: more efficient linux-only transport based on `bluer`
mod transport_btleplug;

use clap::{Parser, Subcommand};
use transport::DfuTransport;

/// Update firmware on nRF BLE DFU targets
#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// BLE DFU target name
    name: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start DFU mode using Buttonless DFU Service
    Trigger {},
    /// Update application only
    App {
        /// DFU package path
        pkg: String,
    },
    /// Update SoftDevice and Bootloader
    Sdbl {
        /// DFU package path
        pkg: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut transport = &mut transport_btleplug::DfuTransportBtleplug::new();
    transport.connect(&args.name).await?;
    match &args.command {
        Commands::Trigger {} => protocol::dfu_trigger(&transport).await,
        Commands::App { pkg } => {
            let (init_pkt, fw_pkt) = package::extract_application(pkg)?;
            protocol::dfu_run(&transport, &init_pkt, &fw_pkt).await
        }
        Commands::Sdbl { pkg } => {
            let (init_pkt, fw_pkt) = package::extract_softdevice_bootloader(pkg)?;
            protocol::dfu_run(&transport, &init_pkt, &fw_pkt).await
        }
    }
}
