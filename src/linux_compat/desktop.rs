//! GNOME/Wayland desktop session through the Linux compatibility layer.
//!
//! RustOS is a Linux userspace environment: Alpine programs in the initramfs run
//! via `fork`/`execve`, `connect(2)` on pre-bound session sockets, and the usual
//! POSIX file/memory/ioctl paths implemented in `linux_compat`.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use super::process_ops;
use super::{LinuxError, LinuxResult};

static DESKTOP_SESSION_READY: AtomicBool = AtomicBool::new(false);

/// Standard GNOME/Wayland session environment for PID 1 and session children.
pub fn default_session_envp() -> Vec<String> {
    super::process_ops::default_session_envp()
}

/// Mark the desktop session path as initialized (linux compat + runtime overlay).
pub fn mark_session_ready() {
    DESKTOP_SESSION_READY.store(true, Ordering::Release);
}

/// True once the desktop session bootstrap path is wired through linux compat.
pub fn is_session_ready() -> bool {
    DESKTOP_SESSION_READY.load(Ordering::Acquire)
}

/// Initialize desktop session support (called from `init_linux_compat`).
pub fn init_desktop_session() {
    mark_session_ready();
    unsafe {
        crate::early_serial_write_str(
            "RustOS: Linux compat desktop session path ready (Wayland/D-Bus via connect)\r\n",
        );
    }
}

/// Load `path` into `pid` using the linux compat ELF loader + shebang resolution.
pub fn exec_program(
    pid: crate::process::Pid,
    path: &str,
    user_argv: &[String],
    extra_envp: &[&str],
) -> LinuxResult<()> {
    process_ops::exec_program_for_pid(pid, path, user_argv, extra_envp)
}

/// Spawn PID 1 session bootstrap (`/bin/init` or `/init`) without leaving kernel context.
///
/// The init script runs as a normal Linux process: syscalls for open/read/connect/fork
/// all route through `linux_compat` while the kernel compositor loop keeps rendering.
pub fn spawn_session_init(path: &str) -> LinuxResult<u32> {
    if !super::is_linux_compat_ready() {
        return Err(LinuxError::ENOSYS);
    }

    use crate::process::scheduler::create_process;
    use crate::process::Priority;

    let pid = create_process(Some(0), Priority::Normal, "init").map_err(|_| LinuxError::EAGAIN)?;

    let argv: Vec<String> = Vec::from([String::from(path)]);
    exec_program(pid, path, &argv, &[])?;

    crate::user_sched::queue_user_pid(pid);
    crate::serial_println!(
        "linux_compat/desktop: spawned {} as PID {} (Alpine userspace on RustOS)",
        path,
        pid
    );
    Ok(pid)
}

/// Verify linux compat can serve a GNOME-style desktop session.
pub fn smoke_check() -> Result<(), &'static str> {
    if !super::is_linux_compat_ready() {
        return Err("linux compat layer is not initialized");
    }
    if !is_session_ready() {
        return Err("desktop session path is not initialized");
    }
    crate::gnome_overlay::smoke_check()?;
    if !crate::wayland::is_ready() {
        return Err("Wayland compositor is not initialized");
    }
    if !crate::dbus::is_ready() {
        return Err("D-Bus session bus is not initialized");
    }
    Ok(())
}
