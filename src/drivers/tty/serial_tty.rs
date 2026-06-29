//! Bridge UART 8250 (COM1) to the console TTY.

extern crate alloc;

use super::{n_tty, CONSOLE_TTY};
use crate::serial;
use core::fmt::Write;

/// Initialize serial-side TTY bridge (COM1 is already init'd by boot).
pub fn init() {}

/// Drain COM1 RX buffer into the console line discipline.
pub fn poll_rx() {
    let data = serial::read_serial1_buffer();
    if data.is_empty() {
        return;
    }

    let mut echo_opt = Some(alloc::vec::Vec::new());
    let mut console = CONSOLE_TTY.lock();
    n_tty::tty_push_input(&mut console, &data, &mut echo_opt);
    drop(console);

    if let Some(echo) = echo_opt {
        if !echo.is_empty() {
            transmit(&echo);
        }
    }
}

/// Write processed bytes to COM1.
pub fn transmit(buf: &[u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let mut port = serial::SERIAL1.lock();
    for &byte in buf {
        let ch = byte as char;
        let _ = port.write_char(ch);
    }
    buf.len()
}

/// Write directly to serial without line discipline (kernel messages).
pub fn write_raw(buf: &[u8]) -> usize {
    transmit(buf)
}
