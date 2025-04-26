use anyhow::Result;

/// nRF DFU transport interface
pub trait DfuTransport {
    /// Connect to the device with the given name
    async fn connect(&mut self, name: &str) -> anyhow::Result<()>;
    /// Write without response
    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<()>;
    /// Subscribe to the given characteristic
    async fn subscribe(&self, char: uuid::Uuid) -> Result<()>;
    /// Write with response then wait for notification response
    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>>;
}
