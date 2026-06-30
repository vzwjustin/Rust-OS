//! HID boot-protocol class driver (keyboard / mouse).
//!
//! Reads interrupt-IN reports from a HID device that was configured in the
//! boot protocol and parses the fixed boot report layouts (USB HID 1.11
//! Appendix B). No HID report-descriptor parsing is required in boot mode.

use alloc::vec::Vec;

use super::super::descriptor::class;
use super::super::hcd::HostController;
use super::super::hub::EnumeratedDevice;

/// Boot HID interface protocol codes (bInterfaceProtocol).
pub const HID_PROTOCOL_KEYBOARD: u8 = 0x01;
pub const HID_PROTOCOL_MOUSE: u8 = 0x02;

/// Parsed boot keyboard report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardReport {
    pub modifiers: u8,
    /// Up to six concurrently pressed key usage codes (0 = empty slot).
    pub keys: [u8; 6],
}

impl KeyboardReport {
    /// Parse the 8-byte boot keyboard report.
    pub fn parse(report: &[u8]) -> Option<KeyboardReport> {
        if report.len() < 8 {
            return None;
        }
        let mut keys = [0u8; 6];
        keys.copy_from_slice(&report[2..8]);
        Some(KeyboardReport {
            modifiers: report[0],
            keys,
        })
    }

    /// First non-zero key usage code, if any.
    pub fn first_key(&self) -> Option<u8> {
        self.keys.iter().copied().find(|k| *k != 0)
    }
}

/// Parsed boot mouse report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseReport {
    pub buttons: u8,
    pub dx: i8,
    pub dy: i8,
    pub wheel: i8,
}

impl MouseReport {
    pub fn parse(report: &[u8]) -> Option<MouseReport> {
        if report.len() < 3 {
            return None;
        }
        Some(MouseReport {
            buttons: report[0],
            dx: report[1] as i8,
            dy: report[2] as i8,
            wheel: report.get(3).map(|w| *w as i8).unwrap_or(0),
        })
    }
}

/// A bound HID boot device.
#[derive(Debug, Clone)]
pub struct HidBootDevice {
    pub slot: u8,
    pub interface_protocol: u8,
    pub interrupt_in_ep: u8,
    pub report_len: u16,
}

/// Match a HID boot-protocol interface on an enumerated device and locate its
/// interrupt-IN endpoint.
pub fn bind(dev: &EnumeratedDevice) -> Option<HidBootDevice> {
    for iface in &dev.config.interfaces {
        if iface.descriptor.interface_class != class::HID {
            continue;
        }
        // Boot subclass (1) with keyboard or mouse protocol.
        if iface.descriptor.interface_subclass != 0x01 {
            continue;
        }
        let ep = iface.endpoints.iter().find(|e| e.is_in() && e.transfer_type() == 3)?;
        return Some(HidBootDevice {
            slot: dev.slot,
            interface_protocol: iface.descriptor.interface_protocol,
            interrupt_in_ep: ep.endpoint_address,
            report_len: ep.max_packet_size.max(8),
        });
    }
    None
}

impl HidBootDevice {
    /// Poll one interrupt-IN report. Returns the raw report bytes that were
    /// transferred (possibly empty if the device NAKed).
    pub fn poll_raw(&self, hc: &mut dyn HostController) -> Result<Vec<u8>, &'static str> {
        let mut buf = alloc::vec![0u8; self.report_len as usize];
        let res = hc.interrupt_transfer(self.slot, self.interrupt_in_ep, &mut buf)?;
        if !res.is_ok() {
            return Err("usb-hid: interrupt transfer failed");
        }
        buf.truncate(res.transferred);
        Ok(buf)
    }

    /// Poll one keyboard report, if this is a keyboard and data was returned.
    pub fn poll_keyboard(
        &self,
        hc: &mut dyn HostController,
    ) -> Result<Option<KeyboardReport>, &'static str> {
        if self.interface_protocol != HID_PROTOCOL_KEYBOARD {
            return Err("usb-hid: not a boot keyboard");
        }
        let raw = self.poll_raw(hc)?;
        if raw.is_empty() {
            return Ok(None);
        }
        Ok(KeyboardReport::parse(&raw))
    }

    /// Poll one mouse report, if this is a mouse and data was returned.
    pub fn poll_mouse(
        &self,
        hc: &mut dyn HostController,
    ) -> Result<Option<MouseReport>, &'static str> {
        if self.interface_protocol != HID_PROTOCOL_MOUSE {
            return Err("usb-hid: not a boot mouse");
        }
        let raw = self.poll_raw(hc)?;
        if raw.is_empty() {
            return Ok(None);
        }
        Ok(MouseReport::parse(&raw))
    }
}
