//! PS/2 Controller (8042) Driver
//!
//! This module implements a driver for the PS/2 controller, also known as the 8042 controller.
//! The PS/2 controller manages PS/2 devices such as keyboards and mice through two channels.
//!
//! Port 1 (Primary): Typically connected to the keyboard
//! Port 2 (Secondary): Typically connected to the mouse
//!
//! I/O Ports:
//! - 0x60: Data port (read/write)
//! - 0x64: Status register (read) / Command register (write)

use spin::Mutex;
use x86_64::instructions::port::Port;

/// PS/2 controller ports
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

/// PS/2 controller commands
const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_DISABLE_PORT2: u8 = 0xA7;
const CMD_ENABLE_PORT2: u8 = 0xA8;
const CMD_TEST_PORT2: u8 = 0xA9;
const CMD_SELF_TEST: u8 = 0xAA;
const CMD_TEST_PORT1: u8 = 0xAB;
const CMD_DISABLE_PORT1: u8 = 0xAD;
const CMD_ENABLE_PORT1: u8 = 0xAE;
const CMD_WRITE_TO_PORT2: u8 = 0xD4;

/// PS/2 device commands
const DEV_ENABLE_SCANNING: u8 = 0xF4;
const DEV_DISABLE_SCANNING: u8 = 0xF5;
const DEV_SET_DEFAULTS: u8 = 0xF6;
const DEV_IDENTIFY: u8 = 0xF2;
const DEV_RESET: u8 = 0xFF;

/// Status register bits
const STATUS_OUTPUT_FULL: u8 = 0x01;
const STATUS_INPUT_FULL: u8 = 0x02;

/// Configuration byte bits
const CONFIG_PORT1_IRQ: u8 = 0x01;
const CONFIG_PORT2_IRQ: u8 = 0x02;
const CONFIG_PORT1_DISABLED: u8 = 0x10;
const CONFIG_PORT2_DISABLED: u8 = 0x20;
const CONFIG_PORT1_TRANSLATION: u8 = 0x40;

/// Expected responses
const SELF_TEST_PASSED: u8 = 0x55;
const PORT_TEST_PASSED: u8 = 0x00;
const ACK: u8 = 0xFA;
const RESEND: u8 = 0xFE;

/// Device identification bytes
const DEVICE_ID_MOUSE: u8 = 0x00;
const DEVICE_ID_SCROLL_MOUSE: u8 = 0x03;
const DEVICE_ID_5BUTTON_MOUSE: u8 = 0x04;

/// Maximum wait iterations for PS/2 operations
const MAX_WAIT_ITERATIONS: usize = 100_000;

/// PS/2 Controller singleton
static PS2_CONTROLLER: Mutex<Option<Ps2Controller>> = Mutex::new(None);

/// PS/2 device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2DeviceType {
    Keyboard,
    StandardMouse,
    MouseWithScrollWheel,
    Mouse5Button,
    Unknown,
}

/// PS/2 port identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2Port {
    Port1,
    Port2,
}

/// PS/2 controller errors
#[derive(Debug, Clone, Copy)]
pub enum Ps2Error {
    Timeout,
    SelfTestFailed,
    PortTestFailed,
    NoSecondPort,
    DeviceError,
    NotInitialized,
}

/// PS/2 Controller driver
pub struct Ps2Controller {
    data_port: Port<u8>,
    status_port: Port<u8>,
    command_port: Port<u8>,
    port1_available: bool,
    port2_available: bool,
    port1_device: Ps2DeviceType,
    port2_device: Ps2DeviceType,
}

impl Ps2Controller {
    /// Create a new uninitialized PS/2 controller
    fn new() -> Self {
        Self {
            data_port: Port::new(PS2_DATA_PORT),
            status_port: Port::new(PS2_STATUS_PORT),
            command_port: Port::new(PS2_COMMAND_PORT),
            port1_available: false,
            port2_available: false,
            port1_device: Ps2DeviceType::Unknown,
            port2_device: Ps2DeviceType::Unknown,
        }
    }

    /// Wait for input buffer to be empty
    fn wait_input_empty(&mut self) -> Result<(), Ps2Error> {
        for _ in 0..MAX_WAIT_ITERATIONS {
            let status = unsafe { self.status_port.read() };
            if (status & STATUS_INPUT_FULL) == 0 {
                return Ok(());
            }
        }
        Err(Ps2Error::Timeout)
    }

    /// Wait for output buffer to be full
    fn wait_output_full(&mut self) -> Result<(), Ps2Error> {
        for _ in 0..MAX_WAIT_ITERATIONS {
            let status = unsafe { self.status_port.read() };
            if (status & STATUS_OUTPUT_FULL) != 0 {
                return Ok(());
            }
        }
        Err(Ps2Error::Timeout)
    }

    /// Read a byte from the data port
    fn read_data(&mut self) -> Result<u8, Ps2Error> {
        self.wait_output_full()?;
        Ok(unsafe { self.data_port.read() })
    }

    /// Write a byte to the data port
    fn write_data(&mut self, data: u8) -> Result<(), Ps2Error> {
        self.wait_input_empty()?;
        unsafe { self.data_port.write(data) };
        Ok(())
    }

    /// Send a command to the controller
    fn send_command(&mut self, command: u8) -> Result<(), Ps2Error> {
        self.wait_input_empty()?;
        unsafe { self.command_port.write(command) };
        Ok(())
    }

    /// Read configuration byte
    fn read_config(&mut self) -> Result<u8, Ps2Error> {
        self.send_command(CMD_READ_CONFIG)?;
        self.read_data()
    }

    /// Write configuration byte
    fn write_config(&mut self, config: u8) -> Result<(), Ps2Error> {
        self.send_command(CMD_WRITE_CONFIG)?;
        self.write_data(config)
    }

    /// Flush the output buffer
    fn flush_output_buffer(&mut self) {
        for _ in 0..16 {
            let status = unsafe { self.status_port.read() };
            if (status & STATUS_OUTPUT_FULL) == 0 {
                break;
            }
            unsafe { self.data_port.read() };
        }
    }

    /// Send a command to a specific port
    fn send_port_command(&mut self, port: Ps2Port, command: u8) -> Result<u8, Ps2Error> {
        if port == Ps2Port::Port2 {
            self.send_command(CMD_WRITE_TO_PORT2)?;
        }
        self.write_data(command)?;

        // Wait for response
        for _ in 0..3 {
            match self.read_data() {
                Ok(response) => {
                    if response == ACK {
                        return Ok(ACK);
                    } else if response == RESEND {
                        return Err(Ps2Error::DeviceError);
                    }
                    return Ok(response);
                }
                Err(_) => continue,
            }
        }

        Err(Ps2Error::Timeout)
    }

    /// Identify device on a port
    fn identify_device(&mut self, port: Ps2Port) -> Ps2DeviceType {
        // Send identify command
        if self.send_port_command(port, DEV_IDENTIFY).is_err() {
            return Ps2DeviceType::Unknown;
        }

        // Try to read identification bytes
        let byte1 = match self.read_data() {
            Ok(b) => b,
            Err(_) => return Ps2DeviceType::Keyboard, // Keyboards often don't respond to identify
        };

        let byte2 = self.read_data().ok();

        // Identify device based on response
        match (byte1, byte2) {
            (DEVICE_ID_MOUSE, None) => Ps2DeviceType::StandardMouse,
            (DEVICE_ID_SCROLL_MOUSE, None) => Ps2DeviceType::MouseWithScrollWheel,
            (DEVICE_ID_5BUTTON_MOUSE, None) => Ps2DeviceType::Mouse5Button,
            _ => {
                // If on port 1 and no proper mouse ID, assume keyboard
                if port == Ps2Port::Port1 {
                    Ps2DeviceType::Keyboard
                } else {
                    Ps2DeviceType::Unknown
                }
            }
        }
    }

    /// Initialize the PS/2 controller
    pub fn initialize(&mut self) -> Result<(), Ps2Error> {
        // Step 1: Disable both PS/2 ports
        self.send_command(CMD_DISABLE_PORT1)?;
        self.send_command(CMD_DISABLE_PORT2)?;

        // Step 2: Flush output buffer
        self.flush_output_buffer();

        // Step 3: Set controller configuration
        let mut config = self.read_config()?;
        let dual_channel = (config & CONFIG_PORT2_DISABLED) != 0;

        // Disable interrupts and translation during initialization
        config &= !(CONFIG_PORT1_IRQ | CONFIG_PORT2_IRQ | CONFIG_PORT1_TRANSLATION);
        self.write_config(config)?;

        // Step 4: Controller self-test
        self.send_command(CMD_SELF_TEST)?;
        let result = self.read_data()?;
        if result != SELF_TEST_PASSED {
            return Err(Ps2Error::SelfTestFailed);
        }

        // Restore configuration (self-test may reset it)
        self.write_config(config)?;

        // Step 5: Determine if dual channel
        if dual_channel {
            self.send_command(CMD_ENABLE_PORT2)?;
            config = self.read_config()?;
            if (config & CONFIG_PORT2_DISABLED) == 0 {
                self.port2_available = true;
                self.send_command(CMD_DISABLE_PORT2)?;
            }
        }

        // Step 6: Test ports
        self.send_command(CMD_TEST_PORT1)?;
        let port1_result = self.read_data()?;
        self.port1_available = port1_result == PORT_TEST_PASSED;

        if self.port2_available {
            self.send_command(CMD_TEST_PORT2)?;
            let port2_result = self.read_data()?;
            self.port2_available = port2_result == PORT_TEST_PASSED;
        }

        if !self.port1_available && !self.port2_available {
            return Err(Ps2Error::PortTestFailed);
        }

        // Step 7: Enable ports
        if self.port1_available {
            self.send_command(CMD_ENABLE_PORT1)?;
        }
        if self.port2_available {
            self.send_command(CMD_ENABLE_PORT2)?;
        }

        // Step 8: Enable interrupts
        let mut config = self.read_config()?;
        if self.port1_available {
            config |= CONFIG_PORT1_IRQ;
        }
        if self.port2_available {
            config |= CONFIG_PORT2_IRQ;
        }
        self.write_config(config)?;

        // Step 9: Identify and reset devices
        if self.port1_available {
            self.port1_device = self.identify_device(Ps2Port::Port1);
        }
        if self.port2_available {
            self.port2_device = self.identify_device(Ps2Port::Port2);
        }

        Ok(())
    }

    /// Get device type on a port
    pub fn get_device_type(&self, port: Ps2Port) -> Ps2DeviceType {
        match port {
            Ps2Port::Port1 => self.port1_device,
            Ps2Port::Port2 => self.port2_device,
        }
    }

    /// Check if a port is available
    pub fn is_port_available(&self, port: Ps2Port) -> bool {
        match port {
            Ps2Port::Port1 => self.port1_available,
            Ps2Port::Port2 => self.port2_available,
        }
    }
}

/// Initialize the PS/2 controller
pub fn init() -> Result<(), Ps2Error> {
    let mut controller = Ps2Controller::new();
    controller.initialize()?;

    // Store in global
    *PS2_CONTROLLER.lock() = Some(controller);

    Ok(())
}

/// Get information about detected PS/2 devices
pub fn get_device_info() -> Option<(bool, Ps2DeviceType, bool, Ps2DeviceType)> {
    let controller = PS2_CONTROLLER.lock();
    if let Some(ref ctrl) = *controller {
        Some((
            ctrl.port1_available,
            ctrl.port1_device,
            ctrl.port2_available,
            ctrl.port2_device,
        ))
    } else {
        None
    }
}

/// Check if PS/2 controller is initialized
pub fn is_initialized() -> bool {
    PS2_CONTROLLER.lock().is_some()
}

/// Send a command to a PS/2 device
pub fn send_device_command(port: Ps2Port, command: u8) -> Result<u8, Ps2Error> {
    let mut controller = PS2_CONTROLLER.lock();
    if let Some(ref mut ctrl) = *controller {
        ctrl.send_port_command(port, command)
    } else {
        Err(Ps2Error::NotInitialized)
    }
}

/// Read a byte from the PS/2 data port (for device drivers)
pub fn read_data_byte() -> Result<u8, Ps2Error> {
    let mut controller = PS2_CONTROLLER.lock();
    if let Some(ref mut ctrl) = *controller {
        ctrl.read_data()
    } else {
        Err(Ps2Error::NotInitialized)
    }
}

/// Write a byte to a PS/2 port (for device drivers)
pub fn write_to_port(port: Ps2Port, data: u8) -> Result<(), Ps2Error> {
    let mut controller = PS2_CONTROLLER.lock();
    if let Some(ref mut ctrl) = *controller {
        if port == Ps2Port::Port2 {
            ctrl.send_command(CMD_WRITE_TO_PORT2)?;
        }
        ctrl.write_data(data)
    } else {
        Err(Ps2Error::NotInitialized)
    }
}
