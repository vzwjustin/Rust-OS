//! GNOME/Wayland desktop session through the Linux compatibility layer.
//!
//! RustOS is a Linux userspace environment: Alpine programs in the initramfs run
//! via `fork`/`execve`, `connect(2)` on pre-bound session sockets, and the usual
//! POSIX file/memory/ioctl paths implemented in `linux_compat`.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use super::process_ops;
use super::{LinuxError, LinuxResult};

static DESKTOP_SESSION_READY: AtomicBool = AtomicBool::new(false);
static GRAPHICAL_TARGET_READY: AtomicBool = AtomicBool::new(false);
static DRM_KMS_READY: AtomicBool = AtomicBool::new(false);
static FRAMEBUFFER_FALLBACK_READY: AtomicBool = AtomicBool::new(false);
static NATIVE_GPU_ACCEL_READY: AtomicBool = AtomicBool::new(false);

/// Userspace session boot intent passed to PID 1 via environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBoot {
    Desktop,
    Install,
    Live,
}

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

/// Record the Linux-style graphical boot target state for PID 1 and desktop services.
pub fn mark_graphical_boot(
    boot: SessionBoot,
    framebuffer_ready: bool,
    drm_kms_ready: bool,
    gpu_accelerated: bool,
    width: usize,
    height: usize,
    bpp: u16,
) {
    let graphical_boot = matches!(boot, SessionBoot::Desktop | SessionBoot::Live);
    let graphical_ready = graphical_boot && framebuffer_ready && bpp >= 24;
    let native_gpu_ready = graphical_ready && drm_kms_ready && gpu_accelerated;
    let framebuffer_fallback = graphical_ready && !native_gpu_ready;

    GRAPHICAL_TARGET_READY.store(graphical_ready, Ordering::Release);
    DRM_KMS_READY.store(drm_kms_ready, Ordering::Release);
    NATIVE_GPU_ACCEL_READY.store(native_gpu_ready, Ordering::Release);
    FRAMEBUFFER_FALLBACK_READY.store(framebuffer_fallback, Ordering::Release);

    let _ = crate::vfs::vfs_mkdir("/run/systemd", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/systemd/system", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/dbus", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/udev", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/udev/data", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/user", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/user/0", 0o700);
    let _ = crate::vfs::vfs_mkdir("/tmp", 0o1777);
    let _ = crate::vfs::vfs_mkdir("/tmp/.X11-unix", 0o1777);

    mark_vfs_file(
        "/run/rustos/graphical.target",
        if graphical_ready {
            b"active\n"
        } else {
            b"inactive\n"
        },
    );
    mark_vfs_file(
        "/run/systemd/system/graphical.target",
        if graphical_ready {
            b"active\n"
        } else {
            b"inactive\n"
        },
    );
    mark_vfs_file(
        "/run/rustos/drm-kms",
        if drm_kms_ready {
            b"ready\n"
        } else {
            b"blocked\n"
        },
    );
    mark_vfs_file(
        "/run/rustos/gpu-acceleration",
        if native_gpu_ready {
            b"native-drm-kms\n"
        } else if gpu_accelerated {
            b"gpu-no-kms\n"
        } else {
            b"software\n"
        },
    );
    mark_vfs_file(
        "/run/rustos/display-stack",
        if native_gpu_ready {
            b"wayland-drm-kms\n"
        } else if framebuffer_fallback {
            b"framebuffer-fallback\n"
        } else {
            b"text\n"
        },
    );
    mark_vfs_file(
        "/run/rustos/framebuffer",
        if framebuffer_ready {
            b"ready\n"
        } else {
            b"unavailable\n"
        },
    );

    crate::serial_println!(
        "linux_compat/desktop: graphical.target={} drm_kms={} native_gpu={} mode={}x{}x{}",
        graphical_ready,
        drm_kms_ready,
        native_gpu_ready,
        width,
        height,
        bpp
    );
}

pub fn is_graphical_target_ready() -> bool {
    GRAPHICAL_TARGET_READY.load(Ordering::Acquire)
}

pub fn is_native_gpu_desktop_ready() -> bool {
    NATIVE_GPU_ACCEL_READY.load(Ordering::Acquire)
}

pub fn is_framebuffer_fallback_ready() -> bool {
    FRAMEBUFFER_FALLBACK_READY.load(Ordering::Acquire)
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
/// Create VFS markers consumed by userspace `/init`.
pub fn mark_session_boot(boot: SessionBoot) {
    let _ = crate::vfs::vfs_mkdir("/run/installer", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/rustos", 0o755);
    match boot {
        SessionBoot::Install => {
            mark_vfs_file("/run/installer/active", b"active\n");
        }
        SessionBoot::Live => {
            mark_vfs_file("/run/rustos/live", b"1\n");
        }
        SessionBoot::Desktop => {
            mark_vfs_file("/run/rustos/desktop", b"1\n");
        }
    }
}

fn mark_vfs_file(path: &str, contents: &[u8]) {
    const O_WRONLY: u32 = 1;
    const O_CREAT: u32 = 64;
    const O_TRUNC: u32 = 512;
    if let Ok(fd) = crate::vfs::vfs_open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644) {
        let _ = crate::vfs::vfs_write(fd, contents);
        let _ = crate::vfs::vfs_close(fd);
    }
}

/// Prepare kernel-side session resources before spawning userspace GNOME/installer.
pub fn prepare_userspace_session() {
    let _ = crate::vfs::vfs_mkdir("/run/user", 0o755);
    let _ = crate::vfs::vfs_mkdir("/run/user/0", 0o700);
    crate::dbus::release_kernel_gnome_stubs();
}

/// Open `/dev/console` on the root filesystem and install it as fd 0, 1, 2
/// on the given PID.  Mirrors Linux's `console_on_rootfs()` which does
/// `sys_open("/dev/console", O_RDWR)` then `sys_dup(0)` twice.
pub fn console_on_rootfs(pid: crate::process::Pid) {
    let pm = crate::process::get_process_manager();
    pm.with_process_mut(pid, |pcb| {
        // Open /dev/console through the kernel VFS manager.
        if let Ok(vfs_fd) =
            crate::fs::vfs().open("/dev/console", crate::fs::OpenFlags::read_write())
        {
            let console_desc = crate::process::FileDescriptor::from_vfs_fd(vfs_fd, 0);
            // Replace the default StandardInput/Output/Error with the real
            // /dev/console VFS handle, matching Linux's fd 0/1/2 setup.
            pcb.file_descriptors.insert(0, console_desc.clone());
            pcb.file_descriptors.insert(1, console_desc.clone());
            pcb.file_descriptors.insert(2, console_desc);
            pcb.fd_table
                .insert(0, crate::process::FileDescriptor::from_vfs_fd(vfs_fd, 0));
            pcb.fd_table
                .insert(1, crate::process::FileDescriptor::from_vfs_fd(vfs_fd, 0));
            pcb.fd_table
                .insert(2, crate::process::FileDescriptor::from_vfs_fd(vfs_fd, 0));
        }
    });
}

pub fn spawn_session_init(path: &str, boot: SessionBoot) -> LinuxResult<u32> {
    if !super::is_linux_compat_ready() {
        return Err(LinuxError::ENOSYS);
    }

    prepare_userspace_session();
    mark_session_boot(boot);

    use crate::process::scheduler::create_process;
    use crate::process::Priority;

    let pid = create_process(Some(0), Priority::Normal, "init").map_err(|_| LinuxError::EAGAIN)?;

    // Open /dev/console as fd 0/1/2 for the init process.
    // Mirrors Linux's console_on_rootfs() in kernel_init_freeable().
    console_on_rootfs(pid);

    let argv: Vec<String> = Vec::from([String::from(path)]);
    let extra: &[&str] = match boot {
        SessionBoot::Install => &["RUSTOS_BOOT=install"],
        SessionBoot::Live => &["RUSTOS_LIVE=1", "RUSTOS_BOOT=live"],
        SessionBoot::Desktop => &[],
    };
    exec_program(pid, path, &argv, extra)?;

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
