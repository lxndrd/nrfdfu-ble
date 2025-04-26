mod package;
mod protocol;
mod transport;
mod transport_btleplug;

use clap::{Parser, Subcommand};

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
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let transport = transport_btleplug::DfuTransportBtleplug::new();
    if let Commands::Trigger {} = &args.command {
        protocol::dfu_trigger(transport, &args.name).await
    } else {
        let (init_pkt, fw_pkt) = match &args.command {
            Commands::App { pkg } => package::extract_application(pkg)?,
            Commands::Sdbl { pkg } => package::extract_softdevice_bootloader(pkg)?,
            _ => unreachable!(),
        };
        protocol::dfu_run(transport, &args.name, &init_pkt, &fw_pkt).await
    }
}
