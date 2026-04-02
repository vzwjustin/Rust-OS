//! Serial Port Driver
//!
//! Basic serial port driver for COM1 and COM2 using UART 16550.

use uart_16550::SerialPort;
use spin::Mutex;
use lazy_static::lazy_static;

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
}

/// Handle serial port 1 interrupt
pub fn handle_port1_interrupt() {
    // Read any available data from COM1
    let mut serial = SERIAL1.lock();

    // Read and discard available data to clear the interrupt
    // uart_16550 receive() returns u8 directly, not Option
    let _byte = serial.receive();
}

/// Handle serial port 2 interrupt
pub fn handle_port2_interrupt() {
    // Read any available data from COM2
    let mut serial = SERIAL2.lock();

    // Read and discard available data to clear the interrupt
    // uart_16550 receive() returns u8 directly, not Option
    let _byte = serial.receive();
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