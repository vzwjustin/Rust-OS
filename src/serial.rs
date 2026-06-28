//! Serial Port Driver
//!
//! Basic serial port driver for COM1 and COM2 using UART 16550.

use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    /// COM1 serial port (0x3F8)
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };

    /// COM2 serial port (0x2F8)
    pub static ref SERIAL2: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x2F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };

    /// Receive buffer for COM1
    static ref SERIAL1_RX: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    /// Receive buffer for COM2
    static ref SERIAL2_RX: Mutex<Vec<u8>> = Mutex::new(Vec::new());
}

/// Read one byte from the serial port and buffer it.
///
/// The UART 16550 interrupt fires when data is available. We read one
/// byte per interrupt call to clear the interrupt. If the FIFO has
/// more data, another interrupt will fire immediately.
fn read_and_buffer(serial: &mut SerialPort, buf: &mut Vec<u8>) {
    let byte = serial.receive();
    buf.push(byte);
}

/// Handle serial port 1 interrupt

pub fn handle_port1_interrupt() {
    let mut serial = SERIAL1.lock();
    let mut rx = SERIAL1_RX.lock();
    read_and_buffer(&mut serial, &mut rx);
}

/// Handle serial port 2 interrupt
pub fn handle_port2_interrupt() {
    let mut serial = SERIAL2.lock();
    let mut rx = SERIAL2_RX.lock();
    read_and_buffer(&mut serial, &mut rx);
}

/// Read buffered bytes from COM1
pub fn read_serial1_buffer() -> Vec<u8> {
    let mut rx = SERIAL1_RX.lock();
    let data = rx.clone();
    rx.clear();
    data
}

/// Read buffered bytes from COM2
pub fn read_serial2_buffer() -> Vec<u8> {
    let mut rx = SERIAL2_RX.lock();
    let data = rx.clone();
    rx.clear();
    data
}

/// Check if COM1 has buffered data
pub fn serial1_has_data() -> bool {
    !SERIAL1_RX.lock().is_empty()
}

/// Check if COM2 has buffered data
pub fn serial2_has_data() -> bool {
    !SERIAL2_RX.lock().is_empty()
}

/// Write formatted arguments to serial port 1
pub fn _print_serial(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let mut serial = SERIAL1.lock();
    let _ = serial.write_fmt(args);
}

/// Serial print macro
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_print_serial(format_args!($($arg)*)));
}

/// Serial println macro
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
