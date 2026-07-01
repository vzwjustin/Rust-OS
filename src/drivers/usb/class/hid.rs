//! HID boot-protocol class driver (keyboard / mouse).
//!
//! Reads interrupt-IN reports from a HID device that was configured in the
//! boot protocol and parses the fixed boot report layouts (USB HID 1.11
//! Appendix B). No HID report-descriptor parsing is required in boot mode.

use alloc::vec::Vec;

use super::super::descriptor::class;
use super::super::hcd::{HostController, SetupPacket};
use super::super::hub::EnumeratedDevice;
use crate::drivers::hid as hid_core;

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
        // HID boot keyboards signal rollover/error by filling the array with
        // usages 1..3. Treat that as no usable report rather than generating
        // spurious presses/releases.
        if report[2..8].iter().any(|usage| (1..=3).contains(usage)) {
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
    pub interface_number: u8,
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
        let ep = iface
            .endpoints
            .iter()
            .find(|e| e.is_in() && e.transfer_type() == 3)?;
        return Some(HidBootDevice {
            slot: dev.slot,
            interface_number: iface.descriptor.interface_number,
            interface_protocol: iface.descriptor.interface_protocol,
            interrupt_in_ep: ep.endpoint_address,
            report_len: ep
                .max_packet_size
                .max(match iface.descriptor.interface_protocol {
                    HID_PROTOCOL_MOUSE => 4,
                    _ => 8,
                }),
        });
    }
    None
}

impl HidBootDevice {
    /// Select HID boot protocol and zero idle rate. This mirrors the generic
    /// usbhid boot path but deliberately skips report-descriptor parsing.
    pub fn configure_boot_protocol(&self, hc: &mut dyn HostController) -> Result<(), &'static str> {
        let set_protocol = SetupPacket {
            request_type: 0x21, // host-to-device, class, interface
            request: 0x0B,      // SET_PROTOCOL
            value: 0,           // boot protocol
            index: self.interface_number as u16,
            length: 0,
        };
        if !hc.control_transfer(self.slot, set_protocol, None)?.is_ok() {
            return Err("usb-hid: SET_PROTOCOL failed");
        }

        let set_idle = SetupPacket {
            request_type: 0x21,
            request: 0x0A, // SET_IDLE
            value: 0,
            index: self.interface_number as u16,
            length: 0,
        };
        if !hc.control_transfer(self.slot, set_idle, None)?.is_ok() {
            return Err("usb-hid: SET_IDLE failed");
        }
        Ok(())
    }

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

    /// Poll and dispatch one boot report into RustOS' HID/input core.
    pub fn poll_and_dispatch(&self, hc: &mut dyn HostController) -> Result<bool, &'static str> {
        let raw = self.poll_raw(hc)?;
        if raw.is_empty() {
            return Ok(false);
        }
        match self.interface_protocol {
            HID_PROTOCOL_KEYBOARD => hid_core::parse_boot_keyboard_report(&raw).map(|_| true),
            HID_PROTOCOL_MOUSE => hid_core::parse_boot_mouse_report(&raw).map(|_| true),
            _ => Err("usb-hid: unsupported boot protocol"),
        }
    }
}
