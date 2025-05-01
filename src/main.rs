mod package;
mod protocol;
mod transport;
mod transport_btleplug;

use clap::{Parser, Subcommand};

/// Update firmware on nRF BLE DFU targets
#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// BLE DFU target name or address
    target: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start DFU mode using Buttonless DFU Service
    Trigger {},
    /// Update application
    App {
        /// DFU package path
        pkg: String,
    },
    /// Update bootloader
    Bl {
        /// DFU package path
        pkg: String,
    },
    /// Update SoftDevice
    Sd {
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
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let transport_manager = transport_btleplug::DfuTransportManagerBtleplug::new().await?;
    if let Commands::Trigger {} = &args.command {
        protocol::dfu_trigger(transport_manager, &args.target).await
    } else {
        let (init_pkt, fw_pkt) = match &args.command {
            Commands::App { pkg } => package::extract_application(pkg)?,
            Commands::Bl { pkg } => package::extract_bootloader(pkg)?,
            Commands::Sd { pkg } => package::extract_softdevice(pkg)?,
            Commands::Sdbl { pkg } => package::extract_softdevice_bootloader(pkg)?,
            _ => unreachable!(),
        };
        protocol::dfu_run(transport_manager, &args.target, &init_pkt, &fw_pkt).await
    }
}
