//! USB Mass Storage class driver — Bulk-Only Transport (BOT).
//!
//! Implements the BOT command/data/status protocol (USB Mass Storage Class
//! Bulk-Only Transport rev 1.0): a Command Block Wrapper (CBW) carries a SCSI
//! command descriptor block, an optional data stage transfers the payload, and
//! a Command Status Wrapper (CSW) reports the result. A handful of SCSI
//! commands are wrapped: INQUIRY, READ CAPACITY(10), READ(10) and WRITE(10).

use alloc::vec;
use alloc::vec::Vec;

use super::super::descriptor::class;
use super::super::hcd::{HostController, TransferDirection};
use super::super::hub::EnumeratedDevice;

const CBW_SIGNATURE: u32 = 0x4342_5355; // 'USBC'
const CSW_SIGNATURE: u32 = 0x5342_5355; // 'USBS'
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;

// SCSI opcodes.
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_READ_CAPACITY10: u8 = 0x25;
const SCSI_READ10: u8 = 0x28;
const SCSI_WRITE10: u8 = 0x2A;

/// Command Status Wrapper as parsed from the bulk-IN status stage.
#[derive(Debug, Clone, Copy)]
pub struct Csw {
    pub signature: u32,
    pub tag: u32,
    pub data_residue: u32,
    pub status: u8,
}

impl Csw {
    fn parse(b: &[u8]) -> Option<Csw> {
        if b.len() < CSW_LEN {
            return None;
        }
        Some(Csw {
            signature: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            tag: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            data_residue: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            status: b[12],
        })
    }

    pub fn is_good(&self, expected_tag: u32) -> bool {
        self.signature == CSW_SIGNATURE && self.tag == expected_tag && self.status == 0
    }
}

/// A bound BOT mass-storage device.
#[derive(Debug, Clone)]
pub struct BotDevice {
    pub slot: u8,
    pub bulk_in_ep: u8,
    pub bulk_out_ep: u8,
    pub block_size: u32,
    pub block_count: u64,
    next_tag: u32,
}

/// Match a Mass Storage / SCSI / Bulk-Only interface and locate its bulk
/// endpoints.
pub fn bind(dev: &EnumeratedDevice) -> Option<BotDevice> {
    for iface in &dev.config.interfaces {
        if iface.descriptor.interface_class != class::MASS_STORAGE {
            continue;
        }
        // SCSI transparent command set (0x06), Bulk-Only protocol (0x50).
        if iface.descriptor.interface_subclass != 0x06
            || iface.descriptor.interface_protocol != 0x50
        {
            continue;
        }
        let bulk_in = iface
            .endpoints
            .iter()
            .find(|e| e.is_in() && e.transfer_type() == 2)?;
        let bulk_out = iface
            .endpoints
            .iter()
            .find(|e| !e.is_in() && e.transfer_type() == 2)?;
        return Some(BotDevice {
            slot: dev.slot,
            bulk_in_ep: bulk_in.endpoint_address,
            bulk_out_ep: bulk_out.endpoint_address,
            block_size: 0,
            block_count: 0,
            next_tag: 1,
        });
    }
    None
}

impl BotDevice {
    fn next_tag(&mut self) -> u32 {
        let tag = self.next_tag;
        self.next_tag = self.next_tag.wrapping_add(1);
        tag
    }

    fn build_cbw(tag: u32, data_len: u32, dir_in: bool, cdb: &[u8]) -> [u8; CBW_LEN] {
        let mut cbw = [0u8; CBW_LEN];
        cbw[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        cbw[4..8].copy_from_slice(&tag.to_le_bytes());
        cbw[8..12].copy_from_slice(&data_len.to_le_bytes());
        cbw[12] = if dir_in { 0x80 } else { 0x00 };
        cbw[13] = 0; // LUN
        cbw[14] = cdb.len().min(16) as u8;
        let n = cdb.len().min(16);
        cbw[15..15 + n].copy_from_slice(&cdb[..n]);
        cbw
    }

    /// Run one full BOT transaction: CBW out, optional data stage, CSW in.
    fn transact(
        &mut self,
        hc: &mut dyn HostController,
        cdb: &[u8],
        data: Option<&mut [u8]>,
        dir_in: bool,
    ) -> Result<(), &'static str> {
        let tag = self.next_tag();
        let data_len = data.as_ref().map(|d| d.len() as u32).unwrap_or(0);

        // Command stage: send the CBW on the bulk-OUT endpoint.
        let mut cbw = Self::build_cbw(tag, data_len, dir_in, cdb);
        let res = hc.bulk_transfer(
            self.slot,
            self.bulk_out_ep,
            TransferDirection::Out,
            &mut cbw,
        )?;
        if !res.is_ok() {
            return Err("usb-bot: CBW transfer failed");
        }

        // Data stage.
        if let Some(buf) = data {
            if data_len != 0 {
                let (ep, dir) = if dir_in {
                    (self.bulk_in_ep, TransferDirection::In)
                } else {
                    (self.bulk_out_ep, TransferDirection::Out)
                };
                let res = hc.bulk_transfer(self.slot, ep, dir, buf)?;
                if !res.is_ok() {
                    return Err("usb-bot: data transfer failed");
                }
            }
        }

        // Status stage: read the CSW on the bulk-IN endpoint.
        let mut csw_buf = [0u8; CSW_LEN];
        let res = hc.bulk_transfer(
            self.slot,
            self.bulk_in_ep,
            TransferDirection::In,
            &mut csw_buf,
        )?;
        if !res.is_ok() {
            return Err("usb-bot: CSW transfer failed");
        }
        let csw = Csw::parse(&csw_buf).ok_or("usb-bot: malformed CSW")?;
        if !csw.is_good(tag) {
            return Err("usb-bot: command failed (bad CSW)");
        }
        Ok(())
    }

    /// SCSI INQUIRY — returns the standard inquiry data.
    pub fn inquiry(&mut self, hc: &mut dyn HostController) -> Result<Vec<u8>, &'static str> {
        let cdb = [SCSI_INQUIRY, 0, 0, 0, 36, 0];
        let mut buf = vec![0u8; 36];
        self.transact(hc, &cdb, Some(&mut buf), true)?;
        Ok(buf)
    }

    /// SCSI READ CAPACITY(10) — caches and returns `(block_count, block_size)`.
    pub fn read_capacity(
        &mut self,
        hc: &mut dyn HostController,
    ) -> Result<(u64, u32), &'static str> {
        let cdb = [SCSI_READ_CAPACITY10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut buf = [0u8; 8];
        self.transact(hc, &cdb, Some(&mut buf), true)?;
        let last_lba = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let block_size = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        self.block_count = last_lba as u64 + 1;
        self.block_size = block_size;
        Ok((self.block_count, self.block_size))
    }

    /// SCSI READ(10) of `count` blocks starting at `lba` into `buf`.
    pub fn read10(
        &mut self,
        hc: &mut dyn HostController,
        lba: u32,
        count: u16,
        buf: &mut [u8],
    ) -> Result<(), &'static str> {
        let cdb = [
            SCSI_READ10,
            0,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            0,
            (count >> 8) as u8,
            count as u8,
            0,
        ];
        self.transact(hc, &cdb, Some(buf), true)
    }

    /// SCSI WRITE(10) of `count` blocks starting at `lba` from `buf`.
    pub fn write10(
        &mut self,
        hc: &mut dyn HostController,
        lba: u32,
        count: u16,
        buf: &mut [u8],
    ) -> Result<(), &'static str> {
        let cdb = [
            SCSI_WRITE10,
            0,
            (lba >> 24) as u8,
            (lba >> 16) as u8,
            (lba >> 8) as u8,
            lba as u8,
            0,
            (count >> 8) as u8,
            count as u8,
            0,
        ];
        self.transact(hc, &cdb, Some(buf), false)
    }
}
