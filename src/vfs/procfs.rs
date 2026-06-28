//! Minimal /proc files for userspace compatibility.

extern crate alloc;

use super::{InodeOps, InodeType, VfsResult};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

fn write_file(dir: &Arc<dyn InodeOps>, name: &str, content: &str) -> VfsResult<()> {
    dir.create(name, InodeType::File, 0o444)?;
    let file = dir.lookup(name)?;
    file.write_at(0, content.as_bytes())?;
    Ok(())
}

fn meminfo_content() -> String {
    if let Ok(stats) = crate::memory_basic::get_memory_stats() {
        let total = stats.usable_memory / 1024;
        let free = stats.usable_memory.saturating_sub(KERNEL_HEAP_SIZE) / 1024;
        format!(
            "MemTotal:       {total} kB\nMemFree:        {free} kB\nMemAvailable:   {free} kB\n"
        )
    } else {
        String::from("MemTotal:       67108864 kB\nMemFree:        64000000 kB\nMemAvailable:   64000000 kB\n")
    }
}

const KERNEL_HEAP_SIZE: usize = crate::memory_basic::KERNEL_HEAP_SIZE;

/// Create /proc with basic files expected by glibc and GTK.
pub fn install_proc(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    root.create("proc", InodeType::Directory, 0o555)?;
    let proc = root.lookup("proc")?;

    write_file(
        &proc,
        "version",
        "Linux version 6.1.0-rustos (rustos@local) (rustc) #1 SMP PREEMPT\n",
    )?;
    write_file(&proc, "meminfo", &meminfo_content())?;
    write_file(
        &proc,
        "cpuinfo",
        "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: RustOS Virtual CPU\n\
         cpu MHz\t\t: 2400.000\ncpu cores\t: 1\n",
    )?;
    write_file(
        &proc,
        "mounts",
        "rootfs / rootfs rw 0 0\nramfs / ramfs rw 0 0\n",
    )?;

    proc.create("self", InodeType::Directory, 0o555)?;
    let self_dir = proc.lookup("self")?;
    write_file(&self_dir, "exe", "/bin/init")?;

    install_rustos(&proc)?;

    Ok(())
}

fn gnome_status_content() -> String {
    let readiness = crate::gnome::probe();
    let mut out = String::new();

    out.push_str("component=gnome\n");
    out.push_str(&format!(
        "overlay={}\n",
        if crate::gnome_overlay::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "dbus={}\n",
        if crate::dbus::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "wayland={}\n",
        if crate::wayland::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "mutter={}\n",
        if crate::mutter::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "foundation_ready={}\n",
        if readiness.foundation_ready() {
            "yes"
        } else {
            "no"
        }
    ));
    out.push_str(&format!(
        "shell_ready={}\n",
        if readiness.gnome_shell_ready() {
            "yes"
        } else {
            "no"
        }
    ));

    if crate::dbus::is_ready() {
        let bus = crate::dbus::bus();
        out.push_str(&format!(
            "name_has_owner_org_freedesktop_DBus={}\n",
            if bus.name_has_owner(crate::dbus::BUS_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "name_has_owner_org_gnome_Shell={}\n",
            if bus.name_has_owner(crate::dbus::GNOME_SHELL_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "name_has_owner_org_rustos_GnomeReadiness={}\n",
            if bus.name_has_owner(crate::dbus::GNOME_READINESS_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
    }

    out
}

fn install_rustos(proc: &Arc<dyn InodeOps>) -> VfsResult<()> {
    proc.create("rustos", InodeType::Directory, 0o555)?;
    let rustos = proc.lookup("rustos")?;
    write_file(&rustos, "gnome", &gnome_status_content())?;
    Ok(())
}

/// Refresh `/proc/rustos/gnome` after subsystem init.
pub fn update_gnome_status() -> VfsResult<()> {
    let root = crate::vfs::get_vfs().lookup("/")?;
    let proc = root.lookup("proc")?;
    let rustos = proc.lookup("rustos")?;
    let file = rustos.lookup("gnome")?;
    file.write_at(0, gnome_status_content().as_bytes())?;
    Ok(())
}
