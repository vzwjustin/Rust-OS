//! GNOME userspace overlay — runtime directories and pre-bound sockets.
//!
//! Prepares the paths GNOME session bootstrap expects before userspace starts:
//! - `XDG_RUNTIME_DIR` (`/run/user/0`)
//! - D-Bus session bus socket (`/run/user/0/bus`)
//! - Wayland compositor socket (`/run/user/0/wayland-0`)

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;

use crate::linux_compat::socket_ops;
use crate::vfs::{self, ramfs::RamFsInode, InodeOps, InodeType, VfsResult};

/// Default UID 0 runtime directory.
pub const RUNTIME_DIR: &str = "/run/user/0";

/// D-Bus session bus socket path used by GNOME and GLib.
pub const DBUS_SESSION_SOCKET: &str = "/run/user/0/bus";

/// Wayland display socket for the first seat.
pub const WAYLAND_SOCKET: &str = "/run/user/0/wayland-0";

/// D-Bus session bus address in standard form.
pub const DBUS_SESSION_ADDRESS: &str = "unix:path=/run/user/0/bus";

static OVERLAY_READY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Returns true once the overlay paths and pre-bound sockets are installed.
pub fn is_ready() -> bool {
    OVERLAY_READY.load(core::sync::atomic::Ordering::Acquire)
}

/// Install GNOME runtime overlay directories and socket nodes under the VFS root.
pub fn install(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    if is_ready() {
        return Ok(());
    }

    let run = ensure_directory(&root, "run", 0o755)?;
    let _dbus_dir = ensure_directory(&run, "dbus", 0o755)?;
    let user = ensure_directory(&run, "user", 0o755)?;
    let runtime = ensure_directory(&user, "0", 0o700)?;

    install_socket(&runtime, "bus", DBUS_SESSION_SOCKET)?;
    install_socket(&runtime, "wayland-0", WAYLAND_SOCKET)?;

    socket_ops::prebind_unix_socket(
        DBUS_SESSION_SOCKET,
        crate::net::unix::UnixSocketRole::DbusSession,
    )
    .map_err(|_| vfs::VfsError::IoError)?;
    socket_ops::prebind_unix_socket(
        WAYLAND_SOCKET,
        crate::net::unix::UnixSocketRole::WaylandDisplay,
    )
    .map_err(|_| vfs::VfsError::IoError)?;

    OVERLAY_READY.store(true, core::sync::atomic::Ordering::Release);

    unsafe {
        crate::early_serial_write_str("RustOS: GNOME runtime overlay installed\r\n");
    }

    Ok(())
}

/// Verify overlay directories and socket nodes are present.
pub fn smoke_check() -> Result<(), &'static str> {
    if !is_ready() {
        return Err("GNOME overlay not installed");
    }

    for path in [RUNTIME_DIR, DBUS_SESSION_SOCKET, WAYLAND_SOCKET] {
        let stat = vfs::vfs_stat(path).map_err(|_| "overlay path missing")?;
        if path == RUNTIME_DIR {
            if stat.inode_type != InodeType::Directory {
                return Err("runtime dir is not a directory");
            }
        } else if stat.inode_type != InodeType::Socket {
            return Err("overlay socket path is not a socket inode");
        }
    }

    if !socket_ops::is_prebound(DBUS_SESSION_SOCKET) {
        return Err("D-Bus session socket not pre-bound");
    }
    if !socket_ops::is_prebound(WAYLAND_SOCKET) {
        return Err("Wayland socket not pre-bound");
    }

    Ok(())
}

fn ensure_directory(
    parent: &Arc<dyn InodeOps>,
    name: &str,
    mode: u32,
) -> VfsResult<Arc<dyn InodeOps>> {
    match parent.lookup(name) {
        Ok(existing) => {
            if existing.stat()?.inode_type != InodeType::Directory {
                return Err(vfs::VfsError::NotDirectory);
            }
            Ok(existing)
        }
        Err(vfs::VfsError::NotFound) => {
            let ino = vfs::get_vfs().alloc_ino();
            let dir = RamFsInode::new_directory(ino, mode);
            parent.attach_child(name, dir.clone() as Arc<dyn InodeOps>)?;
            Ok(dir as Arc<dyn InodeOps>)
        }
        Err(err) => Err(err),
    }
}

fn install_socket(parent: &Arc<dyn InodeOps>, name: &str, full_path: &str) -> VfsResult<()> {
    if parent.lookup(name).is_ok() {
        return Ok(());
    }

    let ino = vfs::get_vfs().alloc_ino();
    let socket = vfs::sockfs::UnixSocketInode::new(ino, full_path);
    parent.attach_child(name, socket as Arc<dyn InodeOps>)
}
