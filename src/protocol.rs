use crate::transport::DfuTransport;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::error::Error;
use std::io::{self, Write};
use std::time::Duration;
use tokio::time::timeout;

// As defined in nRF5_SDK_17.1.0_ddde560/components/libraries/bootloader/dfu/nrf_dfu_req_handler.h

/// DFU Object variants
#[derive(Debug, Copy, Clone, IntoPrimitive)]
#[repr(u8)]
enum Object {
    Command = 0x01,
    Data = 0x02,
}

/// DFU Command opcodes
#[derive(Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
enum OpCode {
    ProtocolVersion = 0x00,
    ObjectCreate = 0x01,
    ReceiptNotifSet = 0x02,
    CrcGet = 0x03,
    ObjectExecute = 0x04,
    ObjectSelect = 0x06,
    MtuGet = 0x07,
    ObjectWrite = 0x08,
    Ping = 0x09,
    HardwareVersion = 0x0A,
    FirmwareVersion = 0x0B,
    Abort = 0x0C,
}

/// DFU Response codes
#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum ResponseCode {
    Invalid = 0x00,
    Success = 0x01,
    OpCodeNotSupported = 0x02,
    InvalidParameter = 0x03,
    InsufficientResources = 0x04,
    InvalidObject = 0x05,
    UnsupportedType = 0x07,
    OperationNotPermitted = 0x08,
    OperationFailed = 0x0A,
    ExtError = 0x0B,
}

fn crc32(buf: &[u8], init: u32) -> u32 {
    let mut h = crc32fast::Hasher::new_with_initial(init);
    h.update(buf);
    h.finalize()
}

// More requests are available when `NRF_DFU_PROTOCOL_REDUCED` is not defined
// in `nRF5_SDK_17.1.0_ddde560/components/libraries/bootloader/dfu/nrf_dfu_req_handler.c`
struct DfuTarget<'a, T: DfuTransport> {
    transport: &'a T,
}

impl<T: DfuTransport> DfuTarget<'_, T> {
    fn verify_header(opcode: u8, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        if bytes.len() < 3 {
            return Err("invalid response length".into());
        }
        if bytes[0] != 0x60 {
            return Err("invalid response header".into());
        }
        if bytes[1] != opcode {
            return Err("invalid response opcode".into());
        }
        let result = ResponseCode::try_from(bytes[2])?;
        if result != ResponseCode::Success {
            return Err(format!("Dfu Target: {:?} ({:?})", result, bytes).into());
        }
        Ok(())
    }

    async fn write_data(&self, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        let write = self.transport.write(dfu_uuids::DATA_PT, bytes);
        timeout(Duration::from_millis(500), write).await?
    }

    async fn request_ctrl(&self, bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        for _retry in 0..3 {
            let request = self.transport.request(dfu_uuids::CTRL_PT, bytes);
            if let Ok(res) = timeout(Duration::from_millis(500), request).await {
                return res;
            }
        }
        Err("No response after multiple tries".into())
    }

    async fn set_prn(&self, value: u32) -> Result<(), Box<dyn Error>> {
        let opcode: u8 = OpCode::ReceiptNotifSet.into();
        let mut payload: Vec<u8> = vec![opcode];
        payload.extend_from_slice(&value.to_le_bytes());
        let response = self.request_ctrl(&payload).await?;
        Self::verify_header(opcode, &response)?;
        Ok(())
    }

    async fn get_crc(&self) -> Result<(usize, u32), Box<dyn Error>> {
        let opcode: u8 = OpCode::CrcGet.into();
        let response = self.request_ctrl(&[opcode]).await?;
        Self::verify_header(opcode, &response)?;
        let offset = u32::from_le_bytes(response[3..7].try_into()?);
        let checksum = u32::from_le_bytes(response[7..11].try_into()?);
        Ok((offset as usize, checksum))
    }

    async fn select_object(&self, obj_type: Object) -> Result<(usize, usize, u32), Box<dyn Error>> {
        let opcode: u8 = OpCode::ObjectSelect.into();
        let arg: u8 = obj_type.into();
        let response = self.request_ctrl(&[opcode, arg]).await?;
        Self::verify_header(opcode, &response)?;
        let max_size = u32::from_le_bytes(response[3..7].try_into()?);
        let offset = u32::from_le_bytes(response[7..11].try_into()?);
        let checksum = u32::from_le_bytes(response[11..15].try_into()?);
        Ok((max_size as usize, offset as usize, checksum))
    }

    async fn create_object(&self, obj_type: Object, len: usize) -> Result<(), Box<dyn Error>> {
        let opcode: u8 = OpCode::ObjectCreate.into();
        let mut payload: Vec<u8> = vec![opcode, obj_type.into()];
        payload.extend_from_slice(&(len as u32).to_le_bytes());
        let response = self.request_ctrl(&payload).await?;
        Self::verify_header(opcode, &response)?;
        Ok(())
    }

    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let opcode: u8 = OpCode::ObjectExecute.into();
        let response = self.request_ctrl(&[opcode]).await?;
        Self::verify_header(opcode, &response)?;
        Ok(())
    }

    async fn verify_crc(&self, offset: usize, checksum: u32) -> Result<(), Box<dyn Error>> {
        let (off, crc) = self.get_crc().await?;
        if offset != off {
            return Err("Length mismatch".into());
        }
        if checksum != crc {
            return Err("CRC mismatch".into());
        }
        Ok(())
    }
}

/// Run DFU procedure as specified in
/// [DFU Protocol](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/lib_dfu_transport_ble.html)
pub async fn dfu_run(transport: &impl DfuTransport, init_pkt: &[u8], fw_pkt: &[u8]) -> Result<(), Box<dyn Error>> {
    let start = std::time::Instant::now();
    // TODO: put timeouts on transport operations
    let target = DfuTarget { transport };
    target.transport.subscribe(dfu_uuids::CTRL_PT).await?;

    // Disable packet receipt notifications
    target.set_prn(0).await?;

    target.create_object(Object::Command, init_pkt.len()).await?;
    target.write_data(init_pkt).await?;
    target.verify_crc(init_pkt.len(), crc32(init_pkt, 0)).await?;
    target.execute().await?;

    let (max_size, offset, checksum) = target.select_object(Object::Data).await?;
    if offset != 0 || checksum != 0 {
        return Err("DFU resumption is not supported".into());
    }
    let mut checksum: u32 = 0;
    let mut offset: usize = 0;
    while offset < fw_pkt.len() {
        let end = std::cmp::min(fw_pkt.len(), offset + max_size);
        let chunk = &fw_pkt[offset..end];
        target.create_object(Object::Data, chunk.len()).await?;
        target.write_data(chunk).await?;
        let new_checksum = crc32(chunk, checksum);
        let new_offset = offset + chunk.len();
        if target.verify_crc(new_offset, new_checksum).await.is_err() {
            println!("CRC error at offset {}, retrying...", offset);
            // first chunk frequently fails on macOS, backoff seems to help
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        checksum = new_checksum;
        offset = new_offset;
        // TODO add progress callback
        let percent = (offset * 100) / fw_pkt.len();
        print!("Uploaded {}% ({}/{} bytes)\r", percent, offset, fw_pkt.len());
        io::stdout().flush().unwrap();
        target.execute().await?;
    }
    println!();
    println!("DFU completed in {:.2} seconds", start.elapsed().as_secs_f32());

    Ok(())
}

/// Trigger DFU mode using the Buttonless DFU service
pub async fn dfu_trigger(transport: &impl DfuTransport) -> Result<(), Box<dyn Error>> {
    transport.subscribe(dfu_uuids::BTTNLSS).await?;
    let res = transport.request(dfu_uuids::BTTNLSS, &[0x01]).await?;
    if res.eq(&[0x20, 0x01, 0x01]) {
        Ok(())
    } else {
        Err("DFU trigger failed".into())
    }
}

/// nRF DFU service & characteristic UUIDs
///
/// from [DFU BLE Service](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/group__nrf__dfu__ble.html)
/// and [Buttonless DFU Service](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/service_dfu.html)
#[allow(dead_code)]
mod dfu_uuids {
    use uuid::Uuid;
    /// DFU Service (16 bit UUID 0xFE59)
    pub const SERVICE: Uuid = Uuid::from_u128(0x0000FE59_0000_1000_8000_00805F9B34FB);
    /// Control Point Characteristic
    pub const CTRL_PT: Uuid = Uuid::from_u128(0x8EC90001_F315_4F60_9FB8_838830DAEA50);
    /// Data Characteristic
    pub const DATA_PT: Uuid = Uuid::from_u128(0x8EC90002_F315_4F60_9FB8_838830DAEA50);
    /// Buttonless DFU trigger without bonds Characteristic
    pub const BTTNLSS: Uuid = Uuid::from_u128(0x8EC90003_F315_4F60_9FB8_838830DAEA50);
    /// Buttonless DFU trigger with bonds Characteristic
    pub const BTTNLSS_WITH_BONDS: Uuid = Uuid::from_u128(0x8EC90004_F315_4F60_9FB8_838830DAEA50);
}
