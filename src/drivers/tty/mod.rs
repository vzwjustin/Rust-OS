//! TTY core: line disciplines, console, and PTY integration.

extern crate alloc;

pub mod n_tty;
pub mod pty;
pub mod serial_tty;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

use crate::linux_compat::{LinuxError, LinuxResult};

/// Input mode flags (Linux termios c_iflag).
pub mod c_iflag {
    pub const IGNBRK: u32 = 0x0001;
    pub const BRKINT: u32 = 0x0002;
    pub const IGNPAR: u32 = 0x0004;
    pub const INLCR: u32 = 0x0040;
    pub const ICRNL: u32 = 0x0100;
    pub const INPCK: u32 = 0x0010;
    pub const ISTRIP: u32 = 0x0020;
    pub const IXON: u32 = 0x0400;
    pub const IXOFF: u32 = 0x1000;
}

/// Output mode flags (Linux termios c_oflag).
pub mod c_oflag {
    pub const OPOST: u32 = 0x0001;
    pub const ONLCR: u32 = 0x0004;
    pub const OCRNL: u32 = 0x0008;
    pub const ONOCR: u32 = 0x0010;
    pub const ONLRET: u32 = 0x0020;
}

/// Control mode flags (Linux termios c_cflag).
pub mod c_cflag {
    pub const CSIZE: u32 = 0x0030;
    pub const CS5: u32 = 0x0000;
    pub const CS6: u32 = 0x0010;
    pub const CS7: u32 = 0x0020;
    pub const CS8: u32 = 0x0030;
    pub const CSTOPB: u32 = 0x0040;
    pub const CREAD: u32 = 0x0080;
    pub const PARENB: u32 = 0x0100;
    pub const PARODD: u32 = 0x0200;
    pub const HUPCL: u32 = 0x0400;
    pub const CLOCAL: u32 = 0x0800;
}

/// Local mode flags (Linux termios c_lflag).
pub mod c_lflag {
    pub const ISIG: u32 = 0x0001;
    pub const ICANON: u32 = 0x0002;
    pub const ECHO: u32 = 0x0008;
    pub const ECHOE: u32 = 0x0010;
    pub const ECHOK: u32 = 0x0020;
    pub const ECHONL: u32 = 0x0040;
    pub const IEXTEN: u32 = 0x8000;
}

/// Control character indices in c_cc.
pub mod cc_index {
    pub const VINTR: usize = 0;
    pub const VQUIT: usize = 1;
    pub const VERASE: usize = 2;
    pub const VKILL: usize = 3;
    pub const VEOF: usize = 4;
    pub const VTIME: usize = 5;
    pub const VMIN: usize = 6;
    pub const VSTART: usize = 8;
    pub const VSTOP: usize = 9;
    pub const VSUSP: usize = 10;
    pub const VEOL: usize = 11;
}

/// Terminal attributes (Linux termios).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line: u8,
    pub c_cc: [u8; 32],
    pub c_ispeed: u32,
    pub c_ospeed: u32,
}

impl Termios {
    pub fn default() -> Self {
        let mut termios = Termios {
            c_iflag: c_iflag::ICRNL | c_iflag::IXON,
            c_oflag: c_oflag::OPOST | c_oflag::ONLCR,
            c_cflag: c_cflag::CREAD | c_cflag::CS8 | c_cflag::HUPCL,
            c_lflag: c_lflag::ISIG
                | c_lflag::ICANON
                | c_lflag::ECHO
                | c_lflag::ECHOE
                | c_lflag::ECHOK,
            c_line: 0,
            c_cc: [0; 32],
            c_ispeed: 38400,
            c_ospeed: 38400,
        };
        termios.c_cc[cc_index::VINTR] = 3;
        termios.c_cc[cc_index::VQUIT] = 28;
        termios.c_cc[cc_index::VERASE] = 127;
        termios.c_cc[cc_index::VKILL] = 21;
        termios.c_cc[cc_index::VEOF] = 4;
        termios.c_cc[cc_index::VSTART] = 17;
        termios.c_cc[cc_index::VSTOP] = 19;
        termios.c_cc[cc_index::VSUSP] = 26;
        termios.c_cc[cc_index::VMIN] = 1;
        termios.c_cc[cc_index::VTIME] = 0;
        termios
    }
}

/// Terminal window size.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

impl WinSize {
    pub const fn default() -> Self {
        WinSize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

/// Identifies a TTY port in the global registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TtyId {
    Console,
    /// Alias of console for /dev/tty.
    Tty,
}

/// Per-port TTY state with line-discipline buffers.
pub struct TtyPort {
    pub termios: Termios,
    pub winsize: WinSize,
    /// Bytes waiting to be read by userspace (post line discipline).
    pub read_buf: Vec<u8>,
    /// Canonical line being assembled (ICANON).
    pub canon_buf: Vec<u8>,
    /// Raw input queue before line processing.
    pub raw_in: Vec<u8>,
    pub output_paused: bool,
    pub input_paused: bool,
}

impl TtyPort {
    pub fn new(termios: Termios) -> Self {
        Self {
            termios,
            winsize: WinSize::default(),
            read_buf: Vec::new(),
            canon_buf: Vec::new(),
            raw_in: Vec::new(),
            output_paused: false,
            input_paused: false,
        }
    }

    pub fn pending_read(&self) -> usize {
        self.read_buf.len()
    }
}

lazy_static! {
    pub(crate) static ref CONSOLE_TTY: Mutex<TtyPort> =
        Mutex::new(TtyPort::new(Termios::default()));
}

static TTY_INIT: AtomicBool = AtomicBool::new(false);

fn with_console<F, R>(f: F) -> R
where
    F: FnOnce(&mut TtyPort) -> R,
{
    f(&mut CONSOLE_TTY.lock())
}

/// Initialize console TTY and serial bridge.
pub fn init() {
    if TTY_INIT.swap(true, Ordering::SeqCst) {
        return;
    }
    with_console(|port| {
        *port = TtyPort::new(Termios::default());
    });
    serial_tty::init();
    pty::init();
}

/// Poll hardware (serial RX) into the console TTY.
pub fn poll_input() {
    serial_tty::poll_rx();
}

pub fn read_console(buf: &mut [u8]) -> usize {
    poll_input();
    let mut echo = None;
    with_console(|port| n_tty::tty_read(port, buf, &mut echo))
}

pub fn write_console(buf: &[u8]) -> usize {
    with_console(|port| {
        let processed = n_tty::process_output(port.termios, buf);
        serial_tty::transmit(&processed)
    })
}

pub fn get_console_termios() -> Termios {
    with_console(|port| port.termios)
}

pub fn set_console_termios(termios: Termios) {
    with_console(|port| port.termios = termios);
}

pub fn get_console_winsize() -> WinSize {
    with_console(|port| port.winsize)
}

pub fn set_console_winsize(winsize: WinSize) {
    with_console(|port| port.winsize = winsize);
}

pub fn flush_console(queue: i32) {
    with_console(|port| n_tty::tty_flush(port, queue));
}

pub fn console_pending_read() -> usize {
    poll_input();
    with_console(|port| port.pending_read())
}

/// Read from a TTY endpoint identified by VFS fd kind or legacy pty fd mapping.
pub fn try_read_fd(fd: i32, buf: &mut [u8]) -> Option<LinuxResult<isize>> {
    if buf.is_empty() {
        return Some(Ok(0));
    }

    if fd >= 0 && fd <= 2 {
        let n = read_console(buf);
        if n == 0 {
            return Some(Err(LinuxError::EAGAIN));
        }
        return Some(Ok(n as isize));
    }

    if let Some(n) = pty::try_read_legacy_fd(fd, buf) {
        return Some(n);
    }

    if let Ok(kind) = crate::vfs::vfs_fd_kind(fd) {
        return match kind {
            crate::vfs::FdKind::TtyConsole => {
                let n = read_console(buf);
                if n == 0 {
                    Some(Err(LinuxError::EAGAIN))
                } else {
                    Some(Ok(n as isize))
                }
            }
            crate::vfs::FdKind::PtyMaster(id) => Some(pty::read_master(id, buf)),
            crate::vfs::FdKind::PtySlave(id) => Some(pty::read_slave(id, buf)),
            _ => None,
        };
    }

    None
}

/// Write to a TTY endpoint.
pub fn try_write_fd(fd: i32, buf: &[u8]) -> Option<LinuxResult<isize>> {
    if buf.is_empty() {
        return Some(Ok(0));
    }

    if fd >= 0 && fd <= 2 {
        return Some(Ok(write_console(buf) as isize));
    }

    if let Some(n) = pty::try_write_legacy_fd(fd, buf) {
        return Some(n);
    }

    if let Ok(kind) = crate::vfs::vfs_fd_kind(fd) {
        return match kind {
            crate::vfs::FdKind::TtyConsole => Some(Ok(write_console(buf) as isize)),
            crate::vfs::FdKind::PtyMaster(id) => Some(pty::write_master(id, buf)),
            crate::vfs::FdKind::PtySlave(id) => Some(pty::write_slave(id, buf)),
            _ => None,
        };
    }

    None
}

/// Read/write for devfs char-device nodes backed by the console TTY.
pub fn devfs_read(_id: TtyId, buf: &mut [u8]) -> usize {
    let _ = _id;
    read_console(buf)
}

pub fn devfs_write(_id: TtyId, buf: &[u8]) -> usize {
    let _ = _id;
    write_console(buf)
}
