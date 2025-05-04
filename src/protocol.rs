use crate::transport::DfuTransport;
use crate::transport::DfuTransportManager;

use anyhow::{Context, Result, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::time::Duration;
use thiserror::Error;
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
#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
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
    Response = 0x60,
}

/// DFU Response codes
#[derive(Error, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum ResponseCode {
    #[error("invalid opcode")]
    Invalid = 0x00,
    #[error("success (not an error)")]
    Success = 0x01,
    #[error("opcode not supported")]
    OpCodeNotSupported = 0x02,
    #[error("invalid parameter")]
    InvalidParameter = 0x03,
    #[error("not enough memory for the data object")]
    InsufficientResources = 0x04,
    #[error("invalid data object")]
    InvalidObject = 0x05,
    #[error("invalid object type")]
    UnsupportedType = 0x07,
    #[error("operation not permitted")]
    OperationNotPermitted = 0x08,
    #[error("operation failed")]
    OperationFailed = 0x0A,
    #[error("extended error")]
    ExtError = 0x0B,
}

/// DFU Extended Error codes
#[derive(Error, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
enum ExtError {
    #[error("no extended error (bad implementation)")]
    NoError = 0x00,
    #[error("invalid error code")]
    InvalidErrorCode = 0x01,
    #[error("wrong command format")]
    WrongCommandFormat = 0x02,
    #[error("unknown command")]
    UnknownCommand = 0x03,
    #[error("invalid init command")]
    InitCommandInvalid = 0x04,
    #[error("firmware version is too low")]
    FwVersionFailure = 0x05,
    #[error("hardware version mismatch")]
    HwVersionFailure = 0x06,
    #[error("required softdevice version mismatch")]
    SdVersionFailure = 0x07,
    #[error("missing signature")]
    SignatureMissing = 0x08,
    #[error("wrong hash type")]
    WrongHashType = 0x09,
    #[error("hash calculation failed")]
    HashFailed = 0x0A,
    #[error("wrong signature type")]
    WrongSignatureType = 0x0B,
    #[error("hash verification failed")]
    VerificationFailed = 0x0C,
    #[error("insufficient space")]
    InsufficientSpace = 0x0D,
}

fn crc32(buf: &[u8], init: u32) -> u32 {
    let mut h = crc32fast::Hasher::new_with_initial(init);
    h.update(buf);
    h.finalize()
}

// More requests are available when `NRF_DFU_PROTOCOL_REDUCED` is not defined
// in `nRF5_SDK_17.1.0_ddde560/components/libraries/bootloader/dfu/nrf_dfu_req_handler.c`
struct DfuTarget<T: DfuTransport> {
    transport: T,
}

impl<T: DfuTransport> DfuTarget<T> {
    fn verify_response(req_opcode: OpCode, bytes: &[u8]) -> Result<()> {
        fn inner(req_opcode: OpCode, bytes: &[u8]) -> Result<()> {
            anyhow::ensure!(bytes.len() >= 3, "invalid response, too short ({:x?})", bytes);
            anyhow::ensure!(bytes[0] == OpCode::Response as u8, "invalid response ({:x?})", bytes);
            anyhow::ensure!(bytes[1] == req_opcode as u8, "invalid request opcode ({:x?})", bytes);
            let result = ResponseCode::try_from(bytes[2])?;
            if result == ResponseCode::ExtError {
                let ext_error = ExtError::try_from(bytes[3])?;
                anyhow::bail!(ext_error);
            }
            if result != ResponseCode::Success {
                anyhow::bail!(result);
            }
            anyhow::Ok(())
        }
        inner(req_opcode, bytes).context(format!("{:?} failed", req_opcode))
    }

    async fn write_data(&self, bytes: &[u8]) -> Result<()> {
        let write = self.transport.write(dfu_uuids::DATA_PT, bytes);
        timeout(Duration::from_millis(500), write).await?
    }

    async fn request_ctrl(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        for _retry in 0..3 {
            let request = self.transport.request(dfu_uuids::CTRL_PT, bytes);
            if let Ok(res) = timeout(Duration::from_millis(500), request).await {
                return res;
            }
        }
        Err(anyhow!("No response after multiple tries"))
    }

    async fn set_prn(&self, value: u32) -> Result<()> {
        let opcode = OpCode::ReceiptNotifSet;
        let mut payload: Vec<u8> = vec![opcode as u8];
        payload.extend_from_slice(&value.to_le_bytes());
        let response = self.request_ctrl(&payload).await?;
        Self::verify_response(opcode, &response)?;
        Ok(())
    }

    async fn get_crc(&self) -> Result<(usize, u32)> {
        let opcode = OpCode::CrcGet;
        let response = self.request_ctrl(&[opcode as u8]).await?;
        Self::verify_response(opcode, &response)?;
        let offset = u32::from_le_bytes(response[3..7].try_into()?);
        let checksum = u32::from_le_bytes(response[7..11].try_into()?);
        Ok((offset as usize, checksum))
    }

    async fn select_object(&self, obj_type: Object) -> Result<(usize, usize, u32)> {
        let opcode = OpCode::ObjectSelect;
        let arg: u8 = obj_type.into();
        let response = self.request_ctrl(&[opcode as u8, arg]).await?;
        Self::verify_response(opcode, &response)?;
        let max_size = u32::from_le_bytes(response[3..7].try_into()?);
        let offset = u32::from_le_bytes(response[7..11].try_into()?);
        let checksum = u32::from_le_bytes(response[11..15].try_into()?);
        Ok((max_size as usize, offset as usize, checksum))
    }

    async fn create_object(&self, obj_type: Object, len: usize) -> Result<()> {
        let opcode = OpCode::ObjectCreate;
        let mut payload: Vec<u8> = vec![opcode as u8, obj_type.into()];
        payload.extend_from_slice(&(len as u32).to_le_bytes());
        let response = self.request_ctrl(&payload).await?;
        Self::verify_response(opcode, &response)?;
        Ok(())
    }

    async fn execute(&self) -> Result<()> {
        let opcode = OpCode::ObjectExecute;
        let response = self.request_ctrl(&[opcode as u8]).await?;
        Self::verify_response(opcode, &response)?;
        Ok(())
    }

    async fn verify_crc(&self, expected_offset: usize, expected_crc: u32) -> Result<()> {
        let (offset, crc) = self.get_crc().await?;
        anyhow::ensure!(expected_offset == offset, "offset mismatch");
        anyhow::ensure!(expected_crc == crc, "CRC mismatch");
        Ok(())
    }
}

/// Run DFU procedure as specified in
/// [DFU Protocol](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/lib_dfu_transport_ble.html)
pub async fn dfu_run<T: DfuTransportManager>(manager: T, name: &str, init_pkt: &[u8], fw_pkt: &[u8]) -> Result<()> {
    let transport = manager.connect(name).await?;
    let target = DfuTarget { transport };
    target.transport.subscribe(dfu_uuids::CTRL_PT).await?;

    let pb = ProgressBar::new(fw_pkt.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("{msg} [{elapsed}] [{wide_bar:.blue/white}] {bytes}/{total_bytes} ({bytes_per_sec})")
            .unwrap()
            .progress_chars("#> "),
    );

    pb.set_message("Uploading...");

    // Disable packet receipt notifications
    target.set_prn(0).await?;

    target.create_object(Object::Command, init_pkt.len()).await?;
    target.write_data(init_pkt).await?;
    target.verify_crc(init_pkt.len(), crc32(init_pkt, 0)).await?;
    target.execute().await?;

    let (max_size, offset, checksum) = target.select_object(Object::Data).await?;
    if offset != 0 || checksum != 0 {
        anyhow::bail!("DFU resumption is not supported");
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
            pb.println(format!("CRC error at offset {}, retrying...", offset));
            // first chunk frequently fails on macOS, backoff seems to help
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        checksum = new_checksum;
        offset = new_offset;
        pb.set_position(offset as u64);
        target.execute().await?;
    }
    pb.finish_with_message("Done");

    Ok(())
}

/// Trigger DFU mode using the Buttonless DFU service
pub async fn dfu_trigger<T: DfuTransportManager>(manager: T, target: &str) -> Result<()> {
    let transport = manager.connect(target).await?;
    transport.subscribe(dfu_uuids::BTTNLSS).await?;
    let res = transport.request(dfu_uuids::BTTNLSS, &[0x01]).await?;
    anyhow::ensure!(res.eq(&[0x20, 0x01, 0x01]), "DFU trigger failed");
    Ok(())
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
