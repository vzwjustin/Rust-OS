//! Terminal and TTY operations
//!
//! This module implements Linux terminal/TTY operations including
//! pseudoterminals (pty), terminal attributes, job control, and line discipline.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use super::process_ops;
use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process;

/// Operation counter for statistics
static TTY_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Next pseudoterminal id.
static NEXT_PTY_ID: AtomicU32 = AtomicU32::new(0);

/// Next synthetic fd for pty endpoints (avoid VFS fd range).
static NEXT_PTY_FD: AtomicI32 = AtomicI32::new(0x4000);

/// In-module PTY registry.
static PTY_REGISTRY: Mutex<BTreeMap<u32, PtyPair>> = Mutex::new(BTreeMap::new());

/// Map fd -> (pty_id, is_master).
static FD_TO_PTY: Mutex<BTreeMap<i32, (u32, bool)>> = Mutex::new(BTreeMap::new());

/// Per-fd termios for tty endpoints (including stdio).
static FD_TERMIOS: Mutex<BTreeMap<i32, Termios>> = Mutex::new(BTreeMap::new());

#[derive(Clone)]
struct PtyPair {
    id: u32,
    termios: Termios,
    winsize: WinSize,
    unlocked: bool,
    granted: bool,
    session_id: Pid,
    foreground_pgrp: Pid,
    input_buf: Vec<u8>,
    output_buf: Vec<u8>,
    output_paused: bool,
    input_paused: bool,
}

impl PtyPair {
    fn new(id: u32) -> Self {
        Self {
            id,
            termios: Termios::default(),
            winsize: WinSize::default(),
            unlocked: false,
            granted: false,
            session_id: process::current_pid() as Pid,
            foreground_pgrp: process::current_pid() as Pid,
            input_buf: Vec::new(),
            output_buf: Vec::new(),
            output_paused: false,
            input_paused: false,
        }
    }
}

fn allocate_pty_fd() -> i32 {
    loop {
        let fd = NEXT_PTY_FD.fetch_add(1, Ordering::SeqCst);
        if fd < 0 {
            continue;
        }
        let map = FD_TO_PTY.lock();
        if !map.contains_key(&fd) {
            return fd;
        }
    }
}

fn create_pty_pair() -> LinuxResult<(u32, i32, i32)> {
    let id = NEXT_PTY_ID.fetch_add(1, Ordering::SeqCst);
    let master_fd = allocate_pty_fd();
    let slave_fd = allocate_pty_fd();

    let pair = PtyPair::new(id);
    PTY_REGISTRY.lock().insert(id, pair);
    FD_TO_PTY.lock().insert(master_fd, (id, true));
    FD_TO_PTY.lock().insert(slave_fd, (id, false));

    Ok((id, master_fd, slave_fd))
}

fn pty_lookup(fd: Fd) -> Option<(u32, bool)> {
    FD_TO_PTY.lock().get(&fd).copied()
}

fn with_pty<F, R>(fd: Fd, f: F) -> LinuxResult<R>
where
    F: FnOnce(&mut PtyPair, bool) -> LinuxResult<R>,
{
    let (pty_id, is_master) = pty_lookup(fd).ok_or(LinuxError::ENOTTY)?;
    let mut registry = PTY_REGISTRY.lock();
    let pair = registry.get_mut(&pty_id).ok_or(LinuxError::ENOTTY)?;
    f(pair, is_master)
}

fn validate_tty_fd(fd: Fd) -> LinuxResult<()> {
    if fd < 0 {
        return Err(LinuxError::EBADF);
    }
    if is_registered_tty_fd(fd) {
        return Ok(());
    }
    Err(LinuxError::ENOTTY)
}

fn termios_for_fd(fd: Fd) -> LinuxResult<Termios> {
    if let Some((pty_id, _)) = pty_lookup(fd) {
        return PTY_REGISTRY
            .lock()
            .get(&pty_id)
            .map(|p| p.termios)
            .ok_or(LinuxError::ENOTTY);
    }
    if fd >= 0 && fd <= 2 {
        return Ok(FD_TERMIOS
            .lock()
            .get(&fd)
            .copied()
            .unwrap_or_else(Termios::default));
    }
    Ok(FD_TERMIOS
        .lock()
        .get(&fd)
        .copied()
        .unwrap_or_else(Termios::default))
}

fn set_termios_for_fd(fd: Fd, termios: Termios) -> LinuxResult<()> {
    if let Some((pty_id, _)) = pty_lookup(fd) {
        if let Some(pair) = PTY_REGISTRY.lock().get_mut(&pty_id) {
            pair.termios = termios;
            return Ok(());
        }
        return Err(LinuxError::ENOTTY);
    }
    if fd >= 0 && fd <= 2 {
        FD_TERMIOS.lock().insert(fd, termios);
        return Ok(());
    }
    FD_TERMIOS.lock().insert(fd, termios);
    Ok(())
}

fn write_slave_name(id: u32, buf: *mut u8, buflen: usize) -> LinuxResult<()> {
    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let name = format!("/dev/pts/{}\0", id);
    if buflen < name.len() {
        return Err(LinuxError::ERANGE);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
    }
    Ok(())
}

/// Returns true when `fd` refers to a registered tty/pty endpoint.
pub fn is_registered_tty_fd(fd: Fd) -> bool {
    if pty_lookup(fd).is_some() {
        return true;
    }
    fd >= 0 && fd <= 2
}

/// Returns true when `fd` is a pseudoterminal endpoint.
pub fn is_pty_fd(fd: Fd) -> bool {
    pty_lookup(fd).is_some()
}

/// Duplicate a tty/pty fd to `newfd`.
pub fn dup_tty_fd(oldfd: Fd, newfd: Fd) -> LinuxResult<Fd> {
    if let Some(mapping) = pty_lookup(oldfd) {
        FD_TO_PTY.lock().insert(newfd, mapping);
        return Ok(newfd);
    }
    if oldfd >= 0 && oldfd <= 2 {
        if let Some(termios) = FD_TERMIOS.lock().get(&oldfd).copied() {
            FD_TERMIOS.lock().insert(newfd, termios);
        }
        return Ok(newfd);
    }
    Err(LinuxError::EBADF)
}

/// Bytes available for read on a tty fd (for FIONREAD).
pub fn tty_pending_read(fd: Fd) -> LinuxResult<usize> {
    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        let registry = PTY_REGISTRY.lock();
        let pair = registry.get(&pty_id).ok_or(LinuxError::ENOTTY)?;
        return Ok(if is_master {
            pair.output_buf.len()
        } else {
            pair.input_buf.len()
        });
    }
    if fd >= 0 && fd <= 2 {
        return Ok(0);
    }
    Err(LinuxError::ENOTTY)
}

/// Get window size for a tty fd.
pub fn tty_get_winsize(fd: Fd) -> LinuxResult<WinSize> {
    if let Some((pty_id, _)) = pty_lookup(fd) {
        return PTY_REGISTRY
            .lock()
            .get(&pty_id)
            .map(|p| p.winsize)
            .ok_or(LinuxError::ENOTTY);
    }
    if fd >= 0 && fd <= 2 {
        return Ok(WinSize::default());
    }
    Err(LinuxError::ENOTTY)
}

/// Set window size for a tty fd.
pub fn tty_set_winsize(fd: Fd, winsize: WinSize) -> LinuxResult<()> {
    with_pty(fd, |pair, _| {
        pair.winsize = winsize;
        Ok(())
    })
    .or_else(|e| {
        if e == LinuxError::ENOTTY && fd >= 0 && fd <= 2 {
            Ok(())
        } else {
            Err(e)
        }
    })
}

/// Initialize TTY operations subsystem
pub fn init_tty_operations() {
    TTY_OPS_COUNT.store(0, Ordering::Relaxed);
    NEXT_PTY_ID.store(0, Ordering::Relaxed);
    NEXT_PTY_FD.store(0x4000, Ordering::SeqCst);
    PTY_REGISTRY.lock().clear();
    FD_TO_PTY.lock().clear();
    FD_TERMIOS.lock().clear();
}

/// Get number of TTY operations performed
pub fn get_operation_count() -> u64 {
    TTY_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    TTY_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// Terminal Attributes (termios)
// ============================================================================

/// Terminal control modes
pub mod c_iflag {
    /// Ignore BREAK condition
    pub const IGNBRK: u32 = 0x0001;
    /// Signal interrupt on BREAK
    pub const BRKINT: u32 = 0x0002;
    /// Ignore characters with parity errors
    pub const IGNPAR: u32 = 0x0004;
    /// Map CR to NL on input
    pub const ICRNL: u32 = 0x0100;
    /// Map NL to CR on input
    pub const INLCR: u32 = 0x0040;
    /// Enable input parity check
    pub const INPCK: u32 = 0x0010;
    /// Strip 8th bit off chars
    pub const ISTRIP: u32 = 0x0020;
    /// Enable XON/XOFF flow control on input
    pub const IXON: u32 = 0x0400;
    /// Enable XON/XOFF flow control on output
    pub const IXOFF: u32 = 0x1000;
}

/// Output modes
pub mod c_oflag {
    /// Post-process output
    pub const OPOST: u32 = 0x0001;
    /// Map NL to CR-NL on output
    pub const ONLCR: u32 = 0x0004;
    /// Map CR to NL on output
    pub const OCRNL: u32 = 0x0008;
    /// No CR output at column 0
    pub const ONOCR: u32 = 0x0010;
    /// NL performs CR function
    pub const ONLRET: u32 = 0x0020;
}

/// Control modes
pub mod c_cflag {
    /// Character size mask
    pub const CSIZE: u32 = 0x0030;
    /// 5 bits
    pub const CS5: u32 = 0x0000;
    /// 6 bits
    pub const CS6: u32 = 0x0010;
    /// 7 bits
    pub const CS7: u32 = 0x0020;
    /// 8 bits
    pub const CS8: u32 = 0x0030;
    /// Send two stop bits
    pub const CSTOPB: u32 = 0x0040;
    /// Enable receiver
    pub const CREAD: u32 = 0x0080;
    /// Parity enable
    pub const PARENB: u32 = 0x0100;
    /// Odd parity
    pub const PARODD: u32 = 0x0200;
    /// Hang up on last close
    pub const HUPCL: u32 = 0x0400;
    /// Ignore modem status lines
    pub const CLOCAL: u32 = 0x0800;
}

/// Local modes
pub mod c_lflag {
    /// Enable echo
    pub const ECHO: u32 = 0x0008;
    /// Echo erase character as error-correcting backspace
    pub const ECHOE: u32 = 0x0010;
    /// Echo KILL character
    pub const ECHOK: u32 = 0x0020;
    /// Echo NL
    pub const ECHONL: u32 = 0x0040;
    /// Enable signals
    pub const ISIG: u32 = 0x0001;
    /// Canonical input (erase and kill processing)
    pub const ICANON: u32 = 0x0002;
    /// Enable extended input processing
    pub const IEXTEN: u32 = 0x8000;
}

/// Special control characters
pub mod cc_index {
    /// End-of-file character
    pub const VEOF: usize = 4;
    /// End-of-line character
    pub const VEOL: usize = 11;
    /// Erase character
    pub const VERASE: usize = 2;
    /// Interrupt character
    pub const VINTR: usize = 0;
    /// Kill-line character
    pub const VKILL: usize = 3;
    /// Minimum number of bytes
    pub const VMIN: usize = 6;
    /// Quit character
    pub const VQUIT: usize = 1;
    /// Start character
    pub const VSTART: usize = 8;
    /// Stop character
    pub const VSTOP: usize = 9;
    /// Suspend character
    pub const VSUSP: usize = 10;
    /// Timeout in deciseconds
    pub const VTIME: usize = 5;
}

/// Terminal attributes structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Termios {
    /// Input modes
    pub c_iflag: u32,
    /// Output modes
    pub c_oflag: u32,
    /// Control modes
    pub c_cflag: u32,
    /// Local modes
    pub c_lflag: u32,
    /// Line discipline
    pub c_line: u8,
    /// Control characters
    pub c_cc: [u8; 32],
    /// Input speed
    pub c_ispeed: u32,
    /// Output speed
    pub c_ospeed: u32,
}

impl Termios {
    /// Create default terminal attributes
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

// ============================================================================
// Terminal Control Operations
// ============================================================================

/// tcgetattr - get terminal attributes
pub fn tcgetattr(fd: Fd, termios_p: *mut Termios) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let termios = termios_for_fd(fd)?;
    unsafe {
        *termios_p = termios;
    }

    Ok(0)
}

/// tcsetattr - set terminal attributes
pub fn tcsetattr(fd: Fd, optional_actions: i32, termios_p: *const Termios) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const TCSANOW: i32 = 0;
    const TCSADRAIN: i32 = 1;
    const TCSAFLUSH: i32 = 2;

    match optional_actions {
        TCSANOW => {
            let termios = unsafe { *termios_p };
            set_termios_for_fd(fd, termios)?;
            Ok(0)
        }
        TCSADRAIN => {
            tcdrain(fd)?;
            let termios = unsafe { *termios_p };
            set_termios_for_fd(fd, termios)?;
            Ok(0)
        }
        TCSAFLUSH => {
            tcflush(fd, 2)?;
            let termios = unsafe { *termios_p };
            set_termios_for_fd(fd, termios)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// tcsendbreak - send break
pub fn tcsendbreak(fd: Fd, _duration: i32) -> LinuxResult<i32> {
    inc_ops();
    validate_tty_fd(fd)?;
    Ok(0)
}

/// tcdrain - wait for output to be transmitted
pub fn tcdrain(fd: Fd) -> LinuxResult<i32> {
    inc_ops();
    with_pty(fd, |pair, is_master| {
        if is_master {
            pair.input_buf.clear();
        } else {
            pair.output_buf.clear();
        }
        Ok(())
    })?;
    validate_tty_fd(fd)?;
    Ok(0)
}

/// tcflush - flush input/output buffers
pub fn tcflush(fd: Fd, queue_selector: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    const TCIFLUSH: i32 = 0;
    const TCOFLUSH: i32 = 1;
    const TCIOFLUSH: i32 = 2;

    match queue_selector {
        TCIFLUSH | TCOFLUSH | TCIOFLUSH => {
            if let Some((pty_id, is_master)) = pty_lookup(fd) {
                let mut registry = PTY_REGISTRY.lock();
                if let Some(pair) = registry.get_mut(&pty_id) {
                    match queue_selector {
                        TCIFLUSH => {
                            if is_master {
                                pair.input_buf.clear();
                            } else {
                                pair.output_buf.clear();
                            }
                        }
                        TCOFLUSH => {
                            if is_master {
                                pair.output_buf.clear();
                            } else {
                                pair.input_buf.clear();
                            }
                        }
                        TCIOFLUSH => {
                            pair.input_buf.clear();
                            pair.output_buf.clear();
                        }
                        _ => {}
                    }
                }
                return Ok(0);
            }
            validate_tty_fd(fd)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// tcflow - suspend/resume transmission or reception
pub fn tcflow(fd: Fd, action: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    const TCOOFF: i32 = 0;
    const TCOON: i32 = 1;
    const TCIOFF: i32 = 2;
    const TCION: i32 = 3;

    match action {
        TCOOFF | TCOON | TCIOFF | TCION => {
            if let Some((pty_id, is_master)) = pty_lookup(fd) {
                let mut registry = PTY_REGISTRY.lock();
                if let Some(pair) = registry.get_mut(&pty_id) {
                    match action {
                        TCOOFF => pair.output_paused = true,
                        TCOON => pair.output_paused = false,
                        TCIOFF => pair.input_paused = true,
                        TCION => pair.input_paused = false,
                        _ => {}
                    }
                }
                let _ = pty_id;
                let _ = is_master;
                return Ok(0);
            }
            validate_tty_fd(fd)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// cfgetispeed - get input baud rate
pub fn cfgetispeed(termios_p: *const Termios) -> u32 {
    inc_ops();

    if termios_p.is_null() {
        return 0;
    }

    unsafe { (*termios_p).c_ispeed }
}

/// cfgetospeed - get output baud rate
pub fn cfgetospeed(termios_p: *const Termios) -> u32 {
    inc_ops();

    if termios_p.is_null() {
        return 0;
    }

    unsafe { (*termios_p).c_ospeed }
}

/// cfsetispeed - set input baud rate
pub fn cfsetispeed(termios_p: *mut Termios, speed: u32) -> LinuxResult<i32> {
    inc_ops();

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    unsafe {
        (*termios_p).c_ispeed = speed;
    }

    Ok(0)
}

/// cfsetospeed - set output baud rate
pub fn cfsetospeed(termios_p: *mut Termios, speed: u32) -> LinuxResult<i32> {
    inc_ops();

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    unsafe {
        (*termios_p).c_ospeed = speed;
    }

    Ok(0)
}

// ============================================================================
// Pseudoterminal Operations
// ============================================================================

/// posix_openpt - open a pseudoterminal device
pub fn posix_openpt(flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    const O_RDWR: i32 = 2;
    const O_NOCTTY: i32 = 0x100;

    if flags & !(O_RDWR | O_NOCTTY) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let (_id, master_fd, _slave_fd) = create_pty_pair()?;
    Ok(master_fd)
}

/// grantpt - grant access to slave pseudoterminal
pub fn grantpt(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    with_pty(fd, |pair, is_master| {
        if !is_master {
            return Err(LinuxError::EINVAL);
        }
        pair.granted = true;
        Ok(())
    })?;
    Ok(0)
}

/// unlockpt - unlock pseudoterminal master/slave pair
pub fn unlockpt(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    with_pty(fd, |pair, is_master| {
        if !is_master {
            return Err(LinuxError::EINVAL);
        }
        pair.unlocked = true;
        Ok(())
    })?;
    Ok(0)
}

/// ptsname - get name of slave pseudoterminal
pub fn ptsname(fd: Fd, buf: *mut u8, buflen: usize) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let (pty_id, is_master) = pty_lookup(fd).ok_or(LinuxError::ENOTTY)?;
    if !is_master {
        return Err(LinuxError::EINVAL);
    }

    let unlocked = PTY_REGISTRY
        .lock()
        .get(&pty_id)
        .map(|p| p.unlocked)
        .unwrap_or(false);
    if !unlocked {
        return Err(LinuxError::EPERM);
    }

    write_slave_name(pty_id, buf, buflen)?;
    Ok(0)
}

/// openpty - open a new pseudoterminal
pub fn openpty(
    amaster: *mut Fd,
    aslave: *mut Fd,
    name: *mut u8,
    termp: *const Termios,
    winp: *const WinSize,
) -> LinuxResult<i32> {
    inc_ops();

    if amaster.is_null() || aslave.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let (pty_id, master_fd, slave_fd) = create_pty_pair()?;

    if !termp.is_null() {
        let termios = unsafe { *termp };
        if let Some(pair) = PTY_REGISTRY.lock().get_mut(&pty_id) {
            pair.termios = termios;
            pair.granted = true;
            pair.unlocked = true;
        }
    } else if let Some(pair) = PTY_REGISTRY.lock().get_mut(&pty_id) {
        pair.granted = true;
        pair.unlocked = true;
    }

    if !winp.is_null() {
        let winsize = unsafe { *winp };
        if let Some(pair) = PTY_REGISTRY.lock().get_mut(&pty_id) {
            pair.winsize = winsize;
        }
    }

    unsafe {
        *amaster = master_fd;
        *aslave = slave_fd;
    }

    if !name.is_null() {
        write_slave_name(pty_id, name, 256)?;
    }

    Ok(0)
}

/// forkpty - fork with new pseudoterminal
pub fn forkpty(
    amaster: *mut Fd,
    name: *mut u8,
    termp: *const Termios,
    winp: *const WinSize,
) -> LinuxResult<Pid> {
    inc_ops();

    if amaster.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut master_fd = 0;
    let mut slave_fd = 0;
    openpty(&mut master_fd, &mut slave_fd, name, termp, winp)?;

    match process_ops::fork() {
        Ok(child_pid) => {
            unsafe {
                *amaster = master_fd;
            }
            let _ = slave_fd;
            Ok(child_pid)
        }
        Err(e) => Err(e),
    }
}

// ============================================================================
// Job Control
// ============================================================================

/// tcgetpgrp - get foreground process group
pub fn tcgetpgrp(fd: Fd) -> LinuxResult<Pid> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if let Some((pty_id, _)) = pty_lookup(fd) {
        return PTY_REGISTRY
            .lock()
            .get(&pty_id)
            .map(|p| p.foreground_pgrp)
            .ok_or(LinuxError::ENOTTY);
    }

    if fd >= 0 && fd <= 2 {
        return Ok(process::current_pid() as Pid);
    }

    Err(LinuxError::ENOTTY)
}

/// tcsetpgrp - set foreground process group
pub fn tcsetpgrp(fd: Fd, pgrp: Pid) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if pgrp <= 0 {
        return Err(LinuxError::EINVAL);
    }

    if let Some((pty_id, _)) = pty_lookup(fd) {
        if let Some(pair) = PTY_REGISTRY.lock().get_mut(&pty_id) {
            pair.foreground_pgrp = pgrp;
            return Ok(0);
        }
        return Err(LinuxError::ENOTTY);
    }

    if fd >= 0 && fd <= 2 {
        return Ok(0);
    }

    Err(LinuxError::ENOTTY)
}

/// tcgetsid - get session ID of terminal
pub fn tcgetsid(fd: Fd) -> LinuxResult<Pid> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if let Some((pty_id, _)) = pty_lookup(fd) {
        return PTY_REGISTRY
            .lock()
            .get(&pty_id)
            .map(|p| p.session_id)
            .ok_or(LinuxError::ENOTTY);
    }

    if fd >= 0 && fd <= 2 {
        return Ok(process::current_pid() as Pid);
    }

    Err(LinuxError::ENOTTY)
}

// ============================================================================
// Terminal Information
// ============================================================================

/// isatty - check if file descriptor refers to a terminal
pub fn isatty(fd: Fd) -> bool {
    inc_ops();
    is_registered_tty_fd(fd)
}

/// ttyname - get terminal name
pub fn ttyname(fd: Fd, buf: *mut u8, buflen: usize) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        if is_master {
            let name = b"/dev/ptmx\0";
            if buflen < name.len() {
                return Err(LinuxError::ERANGE);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
            }
            return Ok(0);
        }
        return write_slave_name(pty_id, buf, buflen).map(|_| 0);
    }

    if fd >= 0 && fd <= 2 {
        let name = b"/dev/tty\0";
        if buflen < name.len() {
            return Err(LinuxError::ERANGE);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
        }
        return Ok(0);
    }

    Err(LinuxError::ENOTTY)
}

/// ctermid - get controlling terminal name
pub fn ctermid(buf: *mut u8) -> *mut u8 {
    inc_ops();

    let name = b"/dev/tty\0";

    if !buf.is_null() {
        unsafe {
            core::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
        }
        buf
    } else {
        name.as_ptr() as *mut u8
    }
}

// ============================================================================
// Window Size
// ============================================================================

/// Window size structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WinSize {
    /// Rows in characters
    pub ws_row: u16,
    /// Columns in characters
    pub ws_col: u16,
    /// Horizontal pixels
    pub ws_xpixel: u16,
    /// Vertical pixels
    pub ws_ypixel: u16,
}

impl WinSize {
    /// Create default window size (80x24)
    pub const fn default() -> Self {
        WinSize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_termios_default() {
        let termios = Termios::default();
        assert_eq!(termios.c_cc[cc_index::VINTR], 3);
        assert_eq!(termios.c_cc[cc_index::VEOF], 4);
        assert!(termios.c_lflag & c_lflag::ECHO != 0);
    }

    #[test_case]
    fn test_tcgetattr() {
        let mut termios = Termios::default().with_default_cc();
        assert!(tcgetattr(0, &mut termios).is_ok());
    }

    #[test_case]
    fn test_isatty() {
        assert!(isatty(0));
        assert!(isatty(1));
        assert!(isatty(2));
        assert!(!isatty(-1));
    }

    #[test_case]
    fn test_openpt() {
        assert!(posix_openpt(2).is_ok());
    }

    #[test_case]
    fn test_winsize() {
        let ws = WinSize::default();
        assert_eq!(ws.ws_row, 24);
        assert_eq!(ws.ws_col, 80);
    }
}
