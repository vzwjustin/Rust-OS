//! Default line discipline (N_TTY): canonical and raw mode from termios.

extern crate alloc;

use alloc::vec::Vec;

use super::{c_iflag, c_lflag, c_oflag, cc_index, Termios, TtyPort};

/// Push raw bytes from hardware/peer into the line discipline.
pub fn tty_push_input(port: &mut TtyPort, data: &[u8], echo_to: &mut Option<Vec<u8>>) {
    if port.input_paused {
        port.raw_in.extend_from_slice(data);
        return;
    }

    for &byte in data {
        if process_input_byte(port, byte, echo_to) {
            break;
        }
    }
}

fn process_input_byte(port: &mut TtyPort, mut byte: u8, echo_to: &mut Option<Vec<u8>>) -> bool {
    let termios = port.termios;
    let canonical = (termios.c_lflag & c_lflag::ICANON) != 0;

    if (termios.c_iflag & c_iflag::ISTRIP) != 0 {
        byte &= 0x7f;
    }

    if byte == b'\r' && (termios.c_iflag & c_iflag::ICRNL) != 0 {
        byte = b'\n';
    }
    if byte == b'\n' && (termios.c_iflag & c_iflag::INLCR) != 0 {
        byte = b'\r';
    }

    if canonical {
        if byte == termios.c_cc[cc_index::VEOF] {
            return true;
        }
        if byte == termios.c_cc[cc_index::VERASE] {
            if !port.canon_buf.is_empty() {
                port.canon_buf.pop();
                maybe_echo(port, b'\x08', echo_to);
                maybe_echo(port, b' ', echo_to);
                maybe_echo(port, b'\x08', echo_to);
            }
            return false;
        }
        if byte == termios.c_cc[cc_index::VKILL] {
            port.canon_buf.clear();
            maybe_echo(port, b'\n', echo_to);
            return false;
        }
        if byte == b'\n' || byte == termios.c_cc[cc_index::VEOL] {
            port.canon_buf.push(byte);
            commit_canonical_line(port);
            return false;
        }
        port.canon_buf.push(byte);
        maybe_echo(port, byte, echo_to);
        return false;
    }

    port.read_buf.push(byte);
    maybe_echo(port, byte, echo_to);
    false
}

fn commit_canonical_line(port: &mut TtyPort) {
    if port.canon_buf.is_empty() {
        return;
    }
    port.read_buf.append(&mut port.canon_buf);
}

fn maybe_echo(port: &TtyPort, byte: u8, echo_to: &mut Option<Vec<u8>>) {
    if (port.termios.c_lflag & c_lflag::ECHO) == 0 {
        return;
    }
    let out = process_output_bytes(port.termios, &[byte]);
    if let Some(buf) = echo_to {
        buf.extend_from_slice(&out);
    }
}

/// Read processed bytes from the TTY read buffer.
pub fn tty_read(port: &mut TtyPort, buf: &mut [u8], echo_to: &mut Option<Vec<u8>>) -> usize {
    if !port.raw_in.is_empty() {
        let pending: Vec<u8> = port.raw_in.drain(..).collect();
        tty_push_input(port, &pending, echo_to);
    }

    if port.read_buf.is_empty() {
        return 0;
    }

    let take = core::cmp::min(buf.len(), port.read_buf.len());
    buf[..take].copy_from_slice(&port.read_buf[..take]);
    port.read_buf.drain(..take);
    take
}

/// Process output bytes according to termios oflag.
pub fn process_output(termios: Termios, buf: &[u8]) -> Vec<u8> {
    process_output_bytes(termios, buf)
}

fn process_output_bytes(termios: Termios, buf: &[u8]) -> Vec<u8> {
    if (termios.c_oflag & c_oflag::OPOST) == 0 {
        return buf.to_vec();
    }

    let mut out = Vec::with_capacity(buf.len() + 8);
    for &byte in buf {
        if byte == b'\n' && (termios.c_oflag & c_oflag::ONLCR) != 0 {
            out.push(b'\r');
            out.push(b'\n');
        } else if byte == b'\r' && (termios.c_oflag & c_oflag::OCRNL) != 0 {
            out.push(b'\n');
        } else {
            out.push(byte);
        }
    }
    out
}

/// Flush input/output queues. queue: 0=iflush, 1=oflush, 2=both.
pub fn tty_flush(port: &mut TtyPort, queue: i32) {
    match queue {
        0 | 2 => {
            port.raw_in.clear();
            port.read_buf.clear();
            port.canon_buf.clear();
        }
        1 => {}
        _ => {}
    }
}

/// Set flow-control pause flags.
pub fn tty_flow(port: &mut TtyPort, action: i32) {
    match action {
        0 => port.output_paused = true,
        1 => port.output_paused = false,
        2 => port.input_paused = true,
        3 => port.input_paused = false,
        _ => {}
    }
}
