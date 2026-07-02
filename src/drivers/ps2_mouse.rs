//! PS/2 Mouse Driver
//!
//! This module implements a driver for PS/2 mice, supporting:
//! - Standard 3-byte PS/2 mouse protocol
//! - IntelliMouse 4-byte protocol (with scroll wheel)
//! - IntelliMouse Explorer 5-button protocol
//!
//! The mouse uses IRQ 12 for interrupts and communicates through the PS/2 controller.

use crate::drivers::ps2_controller::{self, Ps2DeviceType, Ps2Error, Ps2Port};
use spin::Mutex;

/// Mouse packet size for different protocols
const PACKET_SIZE_STANDARD: usize = 3;
const PACKET_SIZE_INTELLIMOUSE: usize = 4;

/// Mouse commands
const MOUSE_CMD_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_CMD_ENABLE: u8 = 0xF4;
const MOUSE_CMD_DISABLE: u8 = 0xF5;
const MOUSE_CMD_SET_SAMPLE_RATE: u8 = 0xF3;
const MOUSE_CMD_GET_DEVICE_ID: u8 = 0xF2;
const MOUSE_CMD_SET_RESOLUTION: u8 = 0xE8;
const MOUSE_CMD_SET_SCALING: u8 = 0xE6;

/// Mouse device IDs
const DEVICE_ID_STANDARD: u8 = 0x00;
const DEVICE_ID_INTELLIMOUSE: u8 = 0x03;
const DEVICE_ID_INTELLIMOUSE_EXPLORER: u8 = 0x04;

/// Packet byte 0 flags
const PACKET0_LEFT_BUTTON: u8 = 0x01;
const PACKET0_RIGHT_BUTTON: u8 = 0x02;
const PACKET0_MIDDLE_BUTTON: u8 = 0x04;
const PACKET0_ALWAYS_ONE: u8 = 0x08;
const PACKET0_X_SIGN: u8 = 0x10;
const PACKET0_Y_SIGN: u8 = 0x20;
const PACKET0_X_OVERFLOW: u8 = 0x40;
const PACKET0_Y_OVERFLOW: u8 = 0x80;

/// Mouse button states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub button4: bool,
    pub button5: bool,
}

impl MouseButtons {
    pub fn new() -> Self {
        Self {
            left: false,
            right: false,
            middle: false,
            button4: false,
            button5: false,
        }
    }
}

/// Mouse packet representing one movement/button event
#[derive(Debug, Clone, Copy)]
pub struct MousePacket {
    pub buttons: MouseButtons,
    pub x_movement: i16,
    pub y_movement: i16,
    pub z_movement: i8, // Scroll wheel
}

impl MousePacket {
    pub fn new() -> Self {
        Self {
            buttons: MouseButtons::new(),
            x_movement: 0,
            y_movement: 0,
            z_movement: 0,
        }
    }
}

/// Mouse protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseProtocol {
    Standard,             // 3-byte packets
    IntelliMouse,         // 4-byte packets with scroll wheel
    IntelliMouseExplorer, // 4-byte packets with 5 buttons
}

/// Packet parser state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    WaitingForByte0,
    WaitingForByte1,
    WaitingForByte2,
    WaitingForByte3,
}

/// PS/2 Mouse driver state
struct MouseState {
    protocol: MouseProtocol,
    parser_state: ParserState,
    packet_buffer: [u8; 4],
    current_buttons: MouseButtons,
    packets_received: usize,
    packets_dropped: usize,
}

impl MouseState {
    fn new() -> Self {
        Self {
            protocol: MouseProtocol::Standard,
            parser_state: ParserState::WaitingForByte0,
            packet_buffer: [0; 4],
            current_buttons: MouseButtons::new(),
            packets_received: 0,
            packets_dropped: 0,
        }
    }

    /// Parse a byte and potentially return a complete packet
    fn parse_byte(&mut self, byte: u8) -> Option<MousePacket> {
        match self.parser_state {
            ParserState::WaitingForByte0 => {
                // Validate sync bit (bit 3 must be 1)
                if (byte & PACKET0_ALWAYS_ONE) != 0 {
                    self.packet_buffer[0] = byte;
                    self.parser_state = ParserState::WaitingForByte1;
                } else {
                    self.packets_dropped += 1;
                }
                None
            }
            ParserState::WaitingForByte1 => {
                self.packet_buffer[1] = byte;
                self.parser_state = ParserState::WaitingForByte2;
                None
            }
            ParserState::WaitingForByte2 => {
                self.packet_buffer[2] = byte;

                if self.protocol == MouseProtocol::Standard {
                    // Standard mouse - 3 bytes complete
                    self.parser_state = ParserState::WaitingForByte0;
                    self.packets_received += 1;
                    Some(self.decode_packet())
                } else {
                    // IntelliMouse - need 4th byte
                    self.parser_state = ParserState::WaitingForByte3;
                    None
                }
            }
            ParserState::WaitingForByte3 => {
                self.packet_buffer[3] = byte;
                self.parser_state = ParserState::WaitingForByte0;
                self.packets_received += 1;
                Some(self.decode_packet())
            }
        }
    }

    /// Decode a complete packet from the buffer
    fn decode_packet(&mut self) -> MousePacket {
        let byte0 = self.packet_buffer[0];
        let byte1 = self.packet_buffer[1];
        let byte2 = self.packet_buffer[2];

        // Decode buttons
        let mut buttons = MouseButtons::new();
        buttons.left = (byte0 & PACKET0_LEFT_BUTTON) != 0;
        buttons.right = (byte0 & PACKET0_RIGHT_BUTTON) != 0;
        buttons.middle = (byte0 & PACKET0_MIDDLE_BUTTON) != 0;

        // Decode movement - handle sign extension
        let x_movement = if (byte0 & PACKET0_X_SIGN) != 0 {
            // Negative value - sign extend
            (byte1 as i16) | (-256i16)
        } else {
            byte1 as i16
        };

        let y_movement = if (byte0 & PACKET0_Y_SIGN) != 0 {
            // Negative value - sign extend
            (byte2 as i16) | (-256i16)
        } else {
            byte2 as i16
        };

        // Check for overflow
        let x_movement = if (byte0 & PACKET0_X_OVERFLOW) != 0 {
            if x_movement >= 0 {
                255
            } else {
                -255
            }
        } else {
            x_movement
        };

        let y_movement = if (byte0 & PACKET0_Y_OVERFLOW) != 0 {
            if y_movement >= 0 {
                255
            } else {
                -255
            }
        } else {
            y_movement
        };

        // Decode scroll wheel (if IntelliMouse)
        let z_movement = if self.protocol != MouseProtocol::Standard {
            let byte3 = self.packet_buffer[3];

            if self.protocol == MouseProtocol::IntelliMouseExplorer {
                // Bits 4-5 contain button 4 and 5
                buttons.button4 = (byte3 & 0x10) != 0;
                buttons.button5 = (byte3 & 0x20) != 0;
            }

            // Lower 4 bits are scroll wheel (sign extended)
            let z = byte3 & 0x0F;
            if z >= 8 {
                (z as i8) | (-16i8)
            } else {
                z as i8
            }
        } else {
            0
        };

        self.current_buttons = buttons;

        MousePacket {
            buttons,
            x_movement,
            y_movement,
            z_movement,
        }
    }

    /// Reset parser state (call when synchronization is lost)
    fn reset_parser(&mut self) {
        self.parser_state = ParserState::WaitingForByte0;
        self.packet_buffer = [0; 4];
    }
}

/// Global mouse state
static MOUSE_STATE: Mutex<Option<MouseState>> = Mutex::new(None);

/// Initialize the PS/2 mouse
pub fn init() -> Result<(), &'static str> {
    // Ensure PS/2 controller is initialized
    if !ps2_controller::is_initialized() {
        return Err("PS/2 controller not initialized");
    }

    // Check if there's a mouse on port 2
    if let Some((_, _, port2_available, port2_device)) = ps2_controller::get_device_info() {
        if !port2_available {
            return Err("PS/2 port 2 not available");
        }

        // Check if it looks like a mouse
        if port2_device != Ps2DeviceType::StandardMouse
            && port2_device != Ps2DeviceType::MouseWithScrollWheel
            && port2_device != Ps2DeviceType::Mouse5Button
        {
            return Err("No mouse detected on PS/2 port 2");
        }
    } else {
        return Err("Could not get PS/2 device info");
    }

    // Initialize mouse state
    let mut state = MouseState::new();

    // Quiesce the device and establish a known baseline before the protocol
    // knock sequences. SET_DEFAULTS resets sample rate/resolution/scaling, so
    // it must precede IntelliMouse detection rather than follow it.
    let _ = send_mouse_command(MOUSE_CMD_DISABLE);
    send_mouse_command(MOUSE_CMD_SET_DEFAULTS).map_err(|_| "Failed to set mouse defaults")?;

    // Linear scaling, 4 counts/mm and 100 reports/sec match Linux psmouse's
    // conservative boot setup.
    send_mouse_command(MOUSE_CMD_SET_SCALING).map_err(|_| "Failed to set mouse scaling")?;
    set_resolution(3)?;
    set_sample_rate(100)?;

    // Try to enable IntelliMouse protocol. The first successful knock changes
    // the ID to 0x03; the Explorer knock then changes it to 0x04 on 5-button
    // devices. Leave standard 3-byte mode if either sequence fails.
    if try_enable_intellimouse().is_ok() {
        state.protocol = MouseProtocol::IntelliMouse;
        if try_enable_intellimouse_explorer().is_ok() {
            state.protocol = MouseProtocol::IntelliMouseExplorer;
        }
    }

    // Restore runtime reporting cadence after the detection sequences.
    set_sample_rate(100)?;
    set_resolution(3)?;

    // Enable mouse data reporting
    send_mouse_command(MOUSE_CMD_ENABLE).map_err(|_| "Failed to enable mouse")?;

    // Store state
    *MOUSE_STATE.lock() = Some(state);

    Ok(())
}

/// Send a command to the mouse
fn send_mouse_command(command: u8) -> Result<u8, Ps2Error> {
    ps2_controller::send_device_command(Ps2Port::Port2, command)
}

/// Set mouse sample rate
fn set_sample_rate(rate: u8) -> Result<(), &'static str> {
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)
        .map_err(|_| "Failed to send set sample rate command")?;
    send_mouse_command(rate).map_err(|_| "Failed to set sample rate value")?;
    Ok(())
}

/// Set mouse resolution (0-3)
fn set_resolution(resolution: u8) -> Result<(), &'static str> {
    send_mouse_command(MOUSE_CMD_SET_RESOLUTION)
        .map_err(|_| "Failed to send set resolution command")?;
    send_mouse_command(resolution & 0x03).map_err(|_| "Failed to set resolution value")?;
    Ok(())
}

/// Try to enable IntelliMouse protocol (magic knock sequence)
fn try_enable_intellimouse() -> Result<(), Ps2Error> {
    // Magic sequence: set sample rate to 200, 100, 80
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(200)?;
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(100)?;
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(80)?;

    // Check device ID
    send_mouse_command(MOUSE_CMD_GET_DEVICE_ID)?;
    let id = ps2_controller::read_data_byte()?;

    if id == DEVICE_ID_INTELLIMOUSE {
        Ok(())
    } else {
        Err(Ps2Error::DeviceError)
    }
}

/// Try to enable IntelliMouse Explorer protocol (5 buttons)
fn try_enable_intellimouse_explorer() -> Result<(), Ps2Error> {
    // Magic sequence: set sample rate to 200, 200, 80
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(200)?;
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(200)?;
    send_mouse_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
    send_mouse_command(80)?;

    // Check device ID
    send_mouse_command(MOUSE_CMD_GET_DEVICE_ID)?;
    let id = ps2_controller::read_data_byte()?;

    if id == DEVICE_ID_INTELLIMOUSE_EXPLORER {
        Ok(())
    } else {
        Err(Ps2Error::DeviceError)
    }
}

/// Process a byte received from the mouse (called from IRQ handler)
///
/// Non-ISR paths (get_button_state/get_statistics/get_protocol/reset_parser)
/// take this same lock with interrupts enabled, so a mouse IRQ arriving on
/// the same CPU while one of those holds it would spin forever on a blocking
/// `lock()`. Use `try_lock()` and drop the byte on contention instead —
/// same fix class as the sysv_ipc timer-ISR deadlock.
pub fn process_byte(byte: u8) -> Option<MousePacket> {
    let mut state = MOUSE_STATE.try_lock()?;
    if let Some(ref mut mouse) = *state {
        mouse.parse_byte(byte)
    } else {
        None
    }
}

/// Get current mouse button state
pub fn get_button_state() -> Option<MouseButtons> {
    let state = MOUSE_STATE.lock();
    state.as_ref().map(|s| s.current_buttons)
}

/// Get mouse statistics
pub fn get_statistics() -> Option<(usize, usize)> {
    let state = MOUSE_STATE.lock();
    state
        .as_ref()
        .map(|s| (s.packets_received, s.packets_dropped))
}

/// Get current mouse protocol
pub fn get_protocol() -> Option<MouseProtocol> {
    let state = MOUSE_STATE.lock();
    state.as_ref().map(|s| s.protocol)
}

/// Reset mouse parser (call if synchronization is lost)
pub fn reset_parser() {
    let mut state = MOUSE_STATE.lock();
    if let Some(ref mut mouse) = *state {
        mouse.reset_parser();
    }
}

/// Check if mouse is initialized
pub fn is_initialized() -> bool {
    MOUSE_STATE.lock().is_some()
}
