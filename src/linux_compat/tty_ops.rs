//! Terminal and TTY operations
//!
//! Syscall-facing API; PTY/line-discipline state lives in `drivers::tty`.

extern crate alloc;

use alloc::format;
use core::sync::atomic::{AtomicU64, Ordering};

use super::process_ops;
use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::drivers::tty::{self, pty};
use crate::memory::user_space::UserSpaceMemory;
use crate::process;

/// Re-export termios types from the TTY driver layer.
pub use crate::drivers::tty::{Termios, WinSize};

/// Operation counter for statistics
static TTY_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

fn pty_lookup(fd: Fd) -> Option<(u32, bool)> {
    pty::legacy_lookup(fd)
}

fn with_pty<F, R>(fd: Fd, f: F) -> LinuxResult<R>
where
    F: FnOnce(u32, bool) -> LinuxResult<R>,
{
    let (pty_id, is_master) = pty_lookup(fd).ok_or(LinuxError::ENOTTY)?;
    f(pty_id, is_master)
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
    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        return pty::get_termios(pty_id, is_master).ok_or(LinuxError::ENOTTY);
    }
    if fd >= 0 && fd <= 2 {
        return Ok(tty::get_console_termios());
    }
    Ok(tty::get_console_termios())
}

fn set_termios_for_fd(fd: Fd, termios: Termios) -> LinuxResult<()> {
    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        return pty::set_termios(pty_id, is_master, termios);
    }
    if fd >= 0 && fd <= 2 {
        tty::set_console_termios(termios);
        return Ok(());
    }
    tty::set_console_termios(termios);
    Ok(())
}

fn write_slave_name(id: u32, buf: *mut u8, buflen: usize) -> LinuxResult<()> {
    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let name = format!("{}\0", pty::slave_name(id));
    if buflen < name.len() {
        return Err(LinuxError::ERANGE);
    }
    copy_bytes_to_user(buf, name.as_bytes())?;
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
    if pty_lookup(oldfd).is_some() {
        return pty::dup_legacy_fd(oldfd, newfd);
    }
    if oldfd >= 0 && oldfd <= 2 {
        return Ok(newfd);
    }
    Err(LinuxError::EBADF)
}

/// Bytes available for read on a tty fd (for FIONREAD).
pub fn tty_pending_read(fd: Fd) -> LinuxResult<usize> {
    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        return Ok(pty::pending_read(pty_id, is_master));
    }
    if fd >= 0 && fd <= 2 {
        return Ok(tty::console_pending_read());
    }
    Err(LinuxError::ENOTTY)
}

/// Get window size for a tty fd.
pub fn tty_get_winsize(fd: Fd) -> LinuxResult<WinSize> {
    if let Some((pty_id, _)) = pty_lookup(fd) {
        return pty::get_winsize(pty_id).ok_or(LinuxError::ENOTTY);
    }
    if fd >= 0 && fd <= 2 {
        return Ok(tty::get_console_winsize());
    }
    Err(LinuxError::ENOTTY)
}

/// Set window size for a tty fd.
pub fn tty_set_winsize(fd: Fd, winsize: WinSize) -> LinuxResult<()> {
    if let Some((pty_id, _)) = pty_lookup(fd) {
        return pty::set_winsize(pty_id, winsize);
    }
    if fd >= 0 && fd <= 2 {
        tty::set_console_winsize(winsize);
        return Ok(());
    }
    Err(LinuxError::ENOTTY)
}

/// Initialize TTY operations subsystem
pub fn init_tty_operations() {
    TTY_OPS_COUNT.store(0, Ordering::Relaxed);
    tty::init();
}

/// Get number of TTY operations performed
pub fn get_operation_count() -> u64 {
    TTY_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    TTY_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn copy_bytes_to_user(dst: *mut u8, bytes: &[u8]) -> LinuxResult<()> {
    if dst.is_null() {
        return Err(LinuxError::EFAULT);
    }
    UserSpaceMemory::copy_to_user(dst as u64, bytes).map_err(|_| LinuxError::EFAULT)
}

fn copy_struct_to_user<T: Copy>(dst: *mut T, value: &T) -> LinuxResult<()> {
    super::copy_struct_to_user(dst, value)
}

fn copy_struct_from_user<T: Copy>(src: *const T) -> LinuxResult<T> {
    super::copy_struct_from_user(src)
}

fn with_termios_mut<R>(
    termios_p: *mut Termios,
    f: impl FnOnce(&mut Termios) -> R,
) -> LinuxResult<R> {
    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let mut termios: Termios = copy_struct_from_user(termios_p)?;
    let result = f(&mut termios);
    copy_struct_to_user(termios_p, &termios)?;
    Ok(result)
}

fn with_termios<R>(termios_p: *const Termios, f: impl FnOnce(&Termios) -> R) -> LinuxResult<R> {
    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let termios: Termios = copy_struct_from_user(termios_p)?;
    Ok(f(&termios))
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
    copy_struct_to_user(termios_p, &termios)?;

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
            let termios: Termios = copy_struct_from_user(termios_p)?;
            set_termios_for_fd(fd, termios)?;
            Ok(0)
        }
        TCSADRAIN => {
            tcdrain(fd)?;
            let termios: Termios = copy_struct_from_user(termios_p)?;
            set_termios_for_fd(fd, termios)?;
            Ok(0)
        }
        TCSAFLUSH => {
            tcflush(fd, 2)?;
            let termios: Termios = copy_struct_from_user(termios_p)?;
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
    if let Some((pty_id, is_master)) = pty_lookup(fd) {
        pty::drain(pty_id, is_master)?;
    } else if fd >= 0 && fd <= 2 {
        // Console output is synchronous on UART.
    } else {
        validate_tty_fd(fd)?;
    }
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
                pty::flush(pty_id, is_master, queue_selector)?;
                return Ok(0);
            }
            if fd >= 0 && fd <= 2 {
                tty::flush_console(queue_selector);
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
                pty::flow(pty_id, is_master, action)?;
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

    with_termios(termios_p, |termios| termios.c_ispeed).unwrap_or(0)
}

/// cfgetospeed - get output baud rate
pub fn cfgetospeed(termios_p: *const Termios) -> u32 {
    inc_ops();

    if termios_p.is_null() {
        return 0;
    }

    with_termios(termios_p, |termios| termios.c_ospeed).unwrap_or(0)
}

/// cfsetispeed - set input baud rate
pub fn cfsetispeed(termios_p: *mut Termios, speed: u32) -> LinuxResult<i32> {
    inc_ops();

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    with_termios_mut(termios_p, |termios| termios.c_ispeed = speed)?;
    Ok(0)
}

/// cfsetospeed - set output baud rate
pub fn cfsetospeed(termios_p: *mut Termios, speed: u32) -> LinuxResult<i32> {
    inc_ops();

    if termios_p.is_null() {
        return Err(LinuxError::EFAULT);
    }

    with_termios_mut(termios_p, |termios| termios.c_ospeed = speed)?;
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

    let (_id, master_fd, _slave_fd) = pty::create_pair()?;
    Ok(master_fd)
}

/// grantpt - grant access to slave pseudoterminal
pub fn grantpt(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    with_pty(fd, |pty_id, is_master| {
        if !is_master {
            return Err(LinuxError::EINVAL);
        }
        pty::set_granted(pty_id)
    })?;
    Ok(0)
}

/// unlockpt - unlock pseudoterminal master/slave pair
pub fn unlockpt(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    with_pty(fd, |pty_id, is_master| {
        if !is_master {
            return Err(LinuxError::EINVAL);
        }
        pty::set_unlocked(pty_id)
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

    if !pty::is_unlocked(pty_id) {
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

    let (pty_id, master_fd, slave_fd) = pty::create_pair()?;

    if !termp.is_null() {
        let termios: Termios = copy_struct_from_user(termp)?;
        pty::set_termios(pty_id, true, termios)?;
        pty::set_termios(pty_id, false, termios)?;
        pty::set_granted(pty_id)?;
        pty::set_unlocked(pty_id)?;
    } else {
        pty::set_granted(pty_id)?;
        pty::set_unlocked(pty_id)?;
    }

    if !winp.is_null() {
        let winsize: WinSize = copy_struct_from_user(winp)?;
        pty::set_winsize(pty_id, winsize)?;
    }

    copy_struct_to_user(amaster, &master_fd)?;
    copy_struct_to_user(aslave, &slave_fd)?;

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
            copy_struct_to_user(amaster, &master_fd)?;
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
        return pty::get_foreground(pty_id)
            .map(|p| p as Pid)
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
        return pty::set_foreground(pty_id, pgrp).map(|_| 0);
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
        return pty::get_session(pty_id)
            .map(|p| p as Pid)
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
            copy_bytes_to_user(buf, name)?;
            return Ok(0);
        }
        return write_slave_name(pty_id, buf, buflen).map(|_| 0);
    }

    if fd >= 0 && fd <= 2 {
        let name = b"/dev/tty\0";
        if buflen < name.len() {
            return Err(LinuxError::ERANGE);
        }
        copy_bytes_to_user(buf, name)?;
        return Ok(0);
    }

    Err(LinuxError::ENOTTY)
}

/// ctermid - get controlling terminal name
pub fn ctermid(buf: *mut u8) -> *mut u8 {
    inc_ops();

    let name = b"/dev/tty\0";

    if !buf.is_null() {
        if copy_bytes_to_user(buf, name).is_ok() {
            buf
        } else {
            core::ptr::null_mut()
        }
    } else {
        name.as_ptr() as *mut u8
    }
}

// ============================================================================
// Tests (disabled)
// ============================================================================

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
