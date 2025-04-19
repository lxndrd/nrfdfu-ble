use crate::transport::DfuTransport;

use btleplug::api::{Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::Adapter;
use btleplug::platform::Peripheral;
use futures::stream::StreamExt;
use std::error::Error;

async fn find_peripheral_by_name(central: &Adapter, name: &str) -> Result<Peripheral, Box<dyn Error>> {
    println!("Searching for {} ...", name);
    central.start_scan(ScanFilter::default()).await?;
    let mut events = central.events().await?;
    while let Some(event) = events.next().await {
        if let CentralEvent::DeviceDiscovered(id) = event {
            let local_name = central.peripheral(&id).await?.properties().await?.unwrap().local_name;
            if let Some(n) = local_name {
                println!("Found [{}] at [{}]", n, id);
                if n == name {
                    central.stop_scan().await?;
                    return Ok(central.peripheral(&id).await?);
                }
            }
        }
    }
    Err("unexpected end of stream".into())
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
    fn characteristic(&self, uuid: uuid::Uuid) -> Result<Characteristic, Box<dyn Error>> {
        // TODO: keep a local char cache for faster lookup
        for char in self.peripheral().characteristics() {
            if uuid == char.uuid {
                return Ok(char);
            }
        }
        Err("characteristic not found".into())
    }
}

impl DfuTransport for &mut DfuTransportBtleplug {
    async fn connect(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let manager = btleplug::platform::Manager::new().await?;
        let adapters = manager.adapters().await?;
        let central = adapters.into_iter().next().unwrap();

        let peripheral = find_peripheral_by_name(&central, name).await?;
        peripheral.connect().await?;
        peripheral.discover_services().await?;

        self.peripheral = Some(peripheral);
        Ok(())
    }
    async fn subscribe(&self, char: uuid::Uuid) -> Result<(), Box<dyn Error>> {
        let char = self.characteristic(char)?;
        self.peripheral().subscribe(&char).await?;
        Ok(())
    }
    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        let char = self.characteristic(char)?;
        self.peripheral()
            .write(&char, bytes, WriteType::WithoutResponse)
            .await?;
        Ok(())
    }
    async fn mtu(&self) -> usize {
        // TODO: btleplug doesn't support MTU discovery
        244
    }
    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
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
