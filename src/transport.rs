use std::error::Error;

/// nRF DFU transport interface
pub trait DfuTransport {
    /// MTU of the BLE link
    async fn mtu(&self) -> usize;
    /// Connect to the device with the given name
    async fn connect(&mut self, name: &str) -> Result<(), Box<dyn Error>>;
    /// Write without response
    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<(), Box<dyn Error>>;
    /// Subscribe to the given characteristic
    async fn subscribe(&self, char: uuid::Uuid) -> Result<(), Box<dyn Error>>;
    /// Write with response then wait for notification response
    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>>;
}
