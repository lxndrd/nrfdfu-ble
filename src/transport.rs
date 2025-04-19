use std::error::Error;

/// nRF DFU service & characteristic UUIDs
///
/// from [DFU BLE Service](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/group__nrf__dfu__ble.html)
/// and [Buttonless DFU Service](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/service_dfu.html)
#[allow(dead_code)]
pub mod dfu_uuids {
    /// DFU Service (16 bit UUID 0xFE59)
    pub const SERVICE: uuid::Uuid = uuid::Uuid::from_u128(0x0000FE59_0000_1000_8000_00805F9B34FB);
    /// Control Point Characteristic
    pub const CTRL_PT: uuid::Uuid = uuid::Uuid::from_u128(0x8EC90001_F315_4F60_9FB8_838830DAEA50);
    /// Data Characteristic
    pub const DATA_PT: uuid::Uuid = uuid::Uuid::from_u128(0x8EC90002_F315_4F60_9FB8_838830DAEA50);
    /// Buttonless DFU trigger without bonds Characteristic
    pub const BTTNLSS: uuid::Uuid = uuid::Uuid::from_u128(0x8EC90003_F315_4F60_9FB8_838830DAEA50);
    /// Buttonless DFU trigger with bonds Characteristic
    pub const BTTNLSS_WITH_BONDS: uuid::Uuid = uuid::Uuid::from_u128(0x8EC90004_F315_4F60_9FB8_838830DAEA50);
}

/// nRF DFU transport interface
pub trait DfuTransport {
    /// MTU of the BLE link
    async fn mtu(&self) -> usize;
    /// Send data to data point
    async fn write_data(&self, bytes: &[u8]) -> Result<(), Box<dyn Error>>;
    /// Exchange request with control point
    async fn request_ctrl(&self, bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>>;
}
