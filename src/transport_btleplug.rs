use crate::transport::DfuTransport;

use anyhow::{Result, anyhow};
use btleplug::api::{
    BDAddr, Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter,
    WriteType,
};
use btleplug::platform::Adapter;
use btleplug::platform::Peripheral;
use futures::stream::StreamExt;
use std::io::Write;
use std::str::FromStr;

async fn find_peripheral<P>(central: &Adapter, predicate: P) -> Result<Peripheral>
where
    P: Fn(PeripheralProperties) -> bool,
{
    central.start_scan(ScanFilter::default()).await?;
    let mut events = central.events().await?;
    while let Some(event) = events.next().await {
        if let CentralEvent::DeviceDiscovered(id) = event {
            let peripheral = central.peripheral(&id).await?;
            if let Some(properties) = peripheral.properties().await? {
                if predicate(properties) {
                    central.stop_scan().await?;
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
async fn find_peripheral_by_address(_central: &Adapter, _addr: &BDAddr) -> Result<Peripheral> {
    Err(anyhow!("BLE MAC addresses are not supported on macOS"))
}

#[cfg(not(target_os = "macos"))]
async fn find_peripheral_by_address(central: &Adapter, addr: &BDAddr) -> Result<Peripheral> {
    println!("Searching for {} by address...", addr);
    find_peripheral(central, |props| {
        print_peripheral_properties(&props);
        props.address_type.is_some() && props.address.eq(addr)
    })
    .await
}
async fn find_peripheral_by_name(central: &Adapter, name: &str) -> Result<Peripheral> {
    println!("Searching for {} by name...", name);
    find_peripheral(central, |props| {
        print_peripheral_properties(&props);
        props.local_name.is_some() && props.local_name.unwrap().eq(name)
    })
    .await
}

pub struct DfuTransportBtleplug {
    peripheral: Option<Peripheral>,
}

impl DfuTransportBtleplug {
    pub fn new() -> Self {
        Self { peripheral: None }
    }
    fn peripheral(&self) -> &Peripheral {
        self.peripheral.as_ref().unwrap()
    }
    fn characteristic(&self, uuid: uuid::Uuid) -> Result<Characteristic> {
        // TODO: keep a local char cache for faster lookup
        for char in self.peripheral().characteristics() {
            if uuid == char.uuid {
                return Ok(char);
            }
        }
        Err(anyhow!("characteristic not found"))
    }
}

impl DfuTransport for DfuTransportBtleplug {
    async fn connect(&mut self, name: &str) -> Result<()> {
        let manager = btleplug::platform::Manager::new().await?;
        let adapters = manager.adapters().await?;
        let central = adapters.into_iter().next().unwrap();

        let peripheral;
        if let Ok(addr) = BDAddr::from_str(name) {
            peripheral = find_peripheral_by_address(&central, &addr).await?;
        } else {
            peripheral = find_peripheral_by_name(&central, name).await?;
        }
        println!();

        peripheral.connect().await?;
        peripheral.discover_services().await?;

        self.peripheral = Some(peripheral);
        Ok(())
    }
    async fn subscribe(&self, char: uuid::Uuid) -> Result<()> {
        let char = self.characteristic(char)?;
        self.peripheral().subscribe(&char).await?;
        Ok(())
    }
    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<()> {
        let char = self.characteristic(char)?;
        // TODO: fix this once btleplug supports MTU discovery
        // default nRF DFU MTU is 244
        const MTU: usize = 244;
        for chunk in bytes.chunks(MTU) {
            self.peripheral()
                .write(&char, chunk, WriteType::WithoutResponse)
                .await?;
        }
        Ok(())
    }
    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut notifications = self.peripheral().notifications().await.unwrap();
        let char = self.characteristic(char)?;
        self.peripheral().write(&char, bytes, WriteType::WithResponse).await?;
        loop {
            let ntf = notifications.next().await.unwrap();
            if ntf.uuid == char.uuid {
                return Ok(ntf.value);
            }
        }
    }
}
