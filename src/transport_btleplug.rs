use crate::transport::{DfuTransport, DfuTransportManager};

use anyhow::{Context, Result, anyhow};
use btleplug::api::{
    BDAddr, Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter, WriteType,
};
use btleplug::platform::Adapter;
use btleplug::platform::Peripheral;
use futures::stream::StreamExt;
use std::io::Write;
use std::str::FromStr;

pub struct DfuTransportManagerBtleplug {
    adapter: Adapter,
}

impl DfuTransportManagerBtleplug {
    pub async fn new() -> anyhow::Result<Self> {
        let manager = btleplug::platform::Manager::new().await?;
        let adapters = manager.adapters().await?;
        if let Some(adapter) = adapters.into_iter().next() {
            Ok(DfuTransportManagerBtleplug { adapter })
        } else {
            Err(anyhow!("No Bluetooth adapter found"))
        }
    }

    async fn find_peripheral<P>(&self, predicate: P) -> Result<Peripheral>
    where
        P: Fn(PeripheralProperties) -> bool,
    {
        self.adapter.start_scan(ScanFilter::default()).await?;
        let mut events = self.adapter.events().await?;
        while let Some(event) = events.next().await {
            if let CentralEvent::DeviceDiscovered(id) = event {
                let peripheral = self.adapter.peripheral(&id).await?;
                if let Some(properties) = peripheral.properties().await? {
                    if predicate(properties) {
                        self.adapter.stop_scan().await?;
                        return Ok(peripheral);
                    }
                }
            }
        }
        Err(anyhow!("Scanning stopped unexpectedly"))
    }

    fn print_peripheral_properties(properties: &PeripheralProperties) {
        let name = properties.local_name.as_deref().unwrap_or("None");
        let addr = properties.address;
        let rssi = properties.rssi.unwrap_or(-99);
        print!("rssi: {}, address: {}, name: {: <32}\r", rssi, addr, name);
        std::io::stdout().flush().unwrap();
    }

    #[cfg(target_os = "macos")]
    async fn find_peripheral_by_address(&self, _addr: &BDAddr) -> Result<Peripheral> {
        Err(anyhow!("BLE MAC addresses are not supported on macOS"))
    }

    #[cfg(not(target_os = "macos"))]
    async fn find_peripheral_by_address(&self, addr: &BDAddr) -> Result<Peripheral> {
        println!("Searching for {} by address...", addr);
        self.find_peripheral(|props| {
            Self::print_peripheral_properties(&props);
            props.address_type.is_some() && props.address.eq(addr)
        })
        .await
    }

    async fn find_peripheral_by_name(&self, name: &str) -> Result<Peripheral> {
        println!("Searching for {} by name...", name);
        self.find_peripheral(|props| {
            Self::print_peripheral_properties(&props);
            props.local_name.is_some() && props.local_name.unwrap().eq(name)
        })
        .await
    }
}

impl DfuTransportManager for DfuTransportManagerBtleplug {
    type Transport = DfuTransportBtleplug;

    async fn connect(&self, target: &str) -> anyhow::Result<Self::Transport> {
        let peripheral;
        if let Ok(addr) = BDAddr::from_str(target) {
            peripheral = self.find_peripheral_by_address(&addr).await?;
        } else {
            peripheral = self.find_peripheral_by_name(target).await?;
        }
        println!();

        peripheral.connect().await.context("Failed to establish a connection")?;
        peripheral.discover_services().await.context("Service discovery failed")?;

        Ok(DfuTransportBtleplug { peripheral })
    }
}

pub struct DfuTransportBtleplug {
    peripheral: Peripheral,
}

impl DfuTransportBtleplug {
    fn characteristic(&self, uuid: uuid::Uuid) -> Result<Characteristic> {
        for char in self.peripheral.characteristics() {
            if uuid == char.uuid {
                return Ok(char);
            }
        }
        Err(anyhow!("characteristic not found"))
    }
}

impl DfuTransport for DfuTransportBtleplug {
    async fn subscribe(&self, char: uuid::Uuid) -> Result<()> {
        let char = self.characteristic(char)?;
        self.peripheral.subscribe(&char).await?;
        Ok(())
    }

    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<()> {
        let char = self.characteristic(char)?;
        // TODO: fix this once btleplug supports MTU discovery
        // default nRF DFU MTU is 244
        const MTU: usize = 244;
        for chunk in bytes.chunks(MTU) {
            self.peripheral.write(&char, chunk, WriteType::WithoutResponse).await?;
        }
        Ok(())
    }

    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut notifications = self.peripheral.notifications().await?;
        let char = self.characteristic(char)?;
        self.peripheral.write(&char, bytes, WriteType::WithResponse).await?;
        while let Some(ntf) = notifications.next().await {
            if ntf.uuid == char.uuid {
                return Ok(ntf.value);
            }
        }
        Err(anyhow!("Notifications stopped unexpectedly"))
    }
}
