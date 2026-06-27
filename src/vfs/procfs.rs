//! Minimal /proc files for userspace compatibility.

extern crate alloc;

use super::{InodeOps, InodeType, VfsError, VfsResult};
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
    write_file(&proc, "mounts", "rootfs / rootfs rw 0 0\nramfs / ramfs rw 0 0\n")?;

    proc.create("self", InodeType::Directory, 0o555)?;
    let self_dir = proc.lookup("self")?;
    write_file(&self_dir, "exe", "/bin/init")?;

    Ok(())
}
