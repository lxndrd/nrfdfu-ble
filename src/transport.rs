use anyhow::Result;

/// DFU transport manager
pub trait DfuTransportManager {
    type Transport: DfuTransport;

    /// Connect to the target device by name or address
    async fn connect(&self, target: &str) -> anyhow::Result<Self::Transport>;
}

/// DFU transport interface
pub trait DfuTransport {
    /// Write without response
    async fn write(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<()>;

    /// Subscribe to the given characteristic
    async fn subscribe(&self, char: uuid::Uuid) -> Result<()>;

    /// Write with response then wait for notification response
    async fn request(&self, char: uuid::Uuid, bytes: &[u8]) -> Result<Vec<u8>>;
}
