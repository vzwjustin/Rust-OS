//! GNOME readiness profile.
//!
//! This is the kernel-side staging contract for GNOME userspace. It does not
//! pretend that gnome-shell can run yet; it reports the real foundation pieces
//! that are present and the platform blockers still needed for a real port.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::{drivers, glib, glib_spawn, vfs};

static BOOT_GRAPHICS_READY: AtomicBool = AtomicBool::new(false);
static GLIB_GIO_READY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GnomeCapabilityState {
    Ready,
    Blocked(&'static str),
}

impl GnomeCapabilityState {
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GnomeReadiness {
    pub glib_gio: GnomeCapabilityState,
    pub posix_spawn: GnomeCapabilityState,
    pub filesystem: GnomeCapabilityState,
    pub graphics: GnomeCapabilityState,
    pub input: GnomeCapabilityState,
    pub linux_abi: GnomeCapabilityState,
    pub dbus: GnomeCapabilityState,
    pub wayland: GnomeCapabilityState,
    pub mutter: GnomeCapabilityState,
    pub drm_kms: GnomeCapabilityState,
}

pub fn mark_boot_graphics_ready() {
    BOOT_GRAPHICS_READY.store(true, Ordering::Release);
}

pub fn mark_glib_gio_ready() {
    GLIB_GIO_READY.store(true, Ordering::Release);
}

impl GnomeReadiness {
    pub const fn foundation_ready(&self) -> bool {
        self.glib_gio.is_ready()
            && self.posix_spawn.is_ready()
            && self.filesystem.is_ready()
            && self.graphics.is_ready()
            && self.input.is_ready()
    }

    pub fn gnome_shell_ready(&self) -> bool {
        self.foundation_ready()
            && self.linux_abi.is_ready()
            && self.dbus.is_ready()
            && self.wayland.is_ready()
            && self.mutter.is_ready()
            && (self.drm_kms.is_ready() || userspace_shell_bridge_active())
    }
}

static USERSPACE_SHELL_BRIDGE: AtomicBool = AtomicBool::new(false);

/// Mark GNOME Shell readiness via kernel compositor + userspace bridge (no DRM/KMS required).
pub fn mark_userspace_shell_bridge() {
    USERSPACE_SHELL_BRIDGE.store(true, Ordering::Release);
}

pub fn userspace_shell_bridge_active() -> bool {
    USERSPACE_SHELL_BRIDGE.load(Ordering::Acquire)
}

pub fn probe() -> GnomeReadiness {
    let driver_status = drivers::get_driver_system_status();
    let graphics_ready = driver_status
        .as_ref()
        .map(|status| status.graphics_ready)
        .unwrap_or(false)
        || BOOT_GRAPHICS_READY.load(Ordering::Acquire);
    let input_ready = driver_status
        .as_ref()
        .map(|status| status.input_ready)
        .unwrap_or(false)
        || crate::drivers::input_manager::is_initialized();

    GnomeReadiness {
        glib_gio: if GLIB_GIO_READY.load(Ordering::Acquire)
            || glib::smoke_check_gnome_readiness().is_ok()
        {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("GLib/GIO smoke path failed")
        },
        posix_spawn: if glib_spawn::spawn_runtime_ready() {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("POSIX process runtime is not ready")
        },
        filesystem: if vfs::vfs_list_dir("/").is_ok() {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("VFS directory enumeration is not ready")
        },
        graphics: if graphics_ready {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("framebuffer/display driver is not ready")
        },
        input: if input_ready {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("keyboard/pointer input stack is not ready")
        },
        linux_abi: if crate::linux_compat::is_linux_compat_ready() {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("Linux userspace ABI is not initialized")
        },
        dbus: if crate::dbus::is_ready() && crate::gnome_overlay::is_ready() {
            GnomeCapabilityState::Ready
        } else if !crate::dbus::is_ready() {
            GnomeCapabilityState::Blocked("D-Bus message bus is not initialized")
        } else {
            GnomeCapabilityState::Blocked("GNOME runtime overlay is not installed")
        },
        wayland: if crate::wayland::is_ready() {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("Wayland compositor is not initialized")
        },
        mutter: if crate::mutter::is_ready() {
            GnomeCapabilityState::Ready
        } else if !crate::gnome_overlay::is_ready() {
            GnomeCapabilityState::Blocked("GNOME runtime overlay is not installed")
        } else if !crate::wayland::is_ready() {
            GnomeCapabilityState::Blocked("Wayland compositor is not initialized")
        } else {
            GnomeCapabilityState::Blocked("Mutter Wayland handshake is not ready")
        },
        drm_kms: if crate::vfs::drmfs::smoke_check().is_ok() {
            GnomeCapabilityState::Ready
        } else {
            GnomeCapabilityState::Blocked("DRM/KMS device nodes are not available")
        },
    }
}

pub fn smoke_check_foundation() -> Result<GnomeReadiness, &'static str> {
    let readiness = probe();

    if !readiness.glib_gio.is_ready() {
        return Err("GNOME requires GLib/GIO");
    }
    if !readiness.posix_spawn.is_ready() {
        return Err("GNOME requires POSIX spawn runtime");
    }
    if !readiness.filesystem.is_ready() {
        return Err("GNOME requires VFS directory enumeration");
    }
    if !readiness.graphics.is_ready() {
        return Err("GNOME requires initialized graphics");
    }

    Ok(readiness)
}

pub fn log_boot_readiness() {
    let readiness = probe();

    // SAFETY: COM1 serial port is initialized.
    unsafe {
        crate::early_serial_write_str("RustOS: GNOME profile probe active\r\n");
        log_capability("GLib/GIO", readiness.glib_gio);
        log_capability("POSIX spawn", readiness.posix_spawn);
        log_capability("filesystem", readiness.filesystem);
        log_capability("graphics", readiness.graphics);
        log_capability("input", readiness.input);
        log_capability("Linux ABI", readiness.linux_abi);
        log_capability("D-Bus", readiness.dbus);
        log_capability("Wayland", readiness.wayland);
        log_capability("Mutter", readiness.mutter);
        log_capability("DRM/KMS", readiness.drm_kms);

        if readiness.gnome_shell_ready() {
            crate::early_serial_write_str("RustOS: GNOME shell launch prerequisites ready\r\n");
        } else if readiness.foundation_ready() {
            crate::early_serial_write_str(
                "RustOS: GNOME foundation ready; shell launch still blocked\r\n",
            );
        } else {
            crate::early_serial_write_str("RustOS: GNOME foundation incomplete\r\n");
        }
    }
}

/// # Safety
/// The caller must ensure COM1 serial is initialized and I/O port
/// access is valid.
unsafe fn log_capability(name: &'static str, state: GnomeCapabilityState) {
    crate::early_serial_write_str("RustOS: GNOME ");
    crate::early_serial_write_str(name);
    match state {
        GnomeCapabilityState::Ready => {
            crate::early_serial_write_str(" ready\r\n");
        }
        GnomeCapabilityState::Blocked(reason) => {
            crate::early_serial_write_str(" blocked: ");
            crate::early_serial_write_str(reason);
            crate::early_serial_write_str("\r\n");
        }
    }
}
