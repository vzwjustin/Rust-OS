//! Linux driver-tree mirror metadata.
//!
//! RustOS mirrors Linux driver source areas as Rust-owned modules first. C shims
//! remain acceptable at ABI or hardware-description boundaries, but this
//! inventory keeps the mirror explicit so gaps are not hidden behind empty
//! modules.

/// Current RustOS disposition for a Linux driver source area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorStatus {
    /// The Linux area is represented by a Rust-owned module with the same path
    /// shape after Rust identifier normalization.
    RustOwned,
    /// The Linux area is represented by an intentionally different RustOS path.
    RustOwnedAlias,
    /// The Linux area still needs a Rust-owned module or an approved C shim.
    PendingRustOwned,
}

/// One Linux driver source-area mirror entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxMirrorEntry {
    /// Linux source-tree path, relative to the Linux root.
    pub linux_path: &'static str,
    /// RustOS path that owns, aliases, or should own the equivalent area.
    pub rustos_path: &'static str,
    /// Current mirror status.
    pub status: MirrorStatus,
    /// Short ownership note.
    pub note: &'static str,
}

/// Linux driver directories that are already covered by Rust-owned aliases.
pub const DRIVER_ALIASES: &[LinuxMirrorEntry] = &[
    LinuxMirrorEntry {
        linux_path: "drivers/base",
        rustos_path: "src/drivers/mod.rs",
        status: MirrorStatus::RustOwnedAlias,
        note: "RustOS driver core and driver-manager facade",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/bluetooth",
        rustos_path: "src/drivers/bt",
        status: MirrorStatus::RustOwnedAlias,
        note: "Rust module uses Linux Bluetooth short name",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/cpufreq",
        rustos_path: "src/cpufreq",
        status: MirrorStatus::RustOwnedAlias,
        note: "CPU frequency subsystem is top-level in RustOS",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/cpuidle",
        rustos_path: "src/cpuidle",
        status: MirrorStatus::RustOwnedAlias,
        note: "CPU idle subsystem is top-level in RustOS",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/dma-buf",
        rustos_path: "src/drivers/dma_buf",
        status: MirrorStatus::RustOwnedAlias,
        note: "Hyphenated Linux directory normalized for Rust identifiers",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/md",
        rustos_path: "src/md",
        status: MirrorStatus::RustOwnedAlias,
        note: "MD RAID/block aggregation is top-level in RustOS",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/net",
        rustos_path: "src/net",
        status: MirrorStatus::RustOwnedAlias,
        note: "Network stack is top-level in RustOS",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/connector",
        rustos_path: "src/drivers/connector",
        status: MirrorStatus::RustOwned,
        note: "Rust-owned connector message bus",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/dca",
        rustos_path: "src/drivers/dca",
        status: MirrorStatus::RustOwned,
        note: "Rust-owned Direct Cache Access provider registry",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/memstick",
        rustos_path: "src/drivers/memstick",
        status: MirrorStatus::RustOwned,
        note: "Rust-owned MemoryStick host/card registry",
    },
    LinuxMirrorEntry {
        linux_path: "drivers/rapidio",
        rustos_path: "src/drivers/rapidio",
        status: MirrorStatus::RustOwned,
        note: "Rust-owned RapidIO fabric registry",
    },
];

/// Linux driver directories with RustOS-owned mirrors.
pub const RUST_OWNED_DRIVER_MIRRORS: &[LinuxMirrorEntry] = &[
    rust_owned(
        "drivers/accel",
        "src/drivers/accel",
        "Rust-owned accelerator device registry and software submit path",
    ),
    rust_owned(
        "drivers/android",
        "src/drivers/android",
        "Rust-owned Binder process/node transaction model and ashmem registry",
    ),
    rust_owned(
        "drivers/atm",
        "src/drivers/atm",
        "Rust-owned ATM device and VCC cell accounting registry",
    ),
    rust_owned(
        "drivers/bcma",
        "src/drivers/bcma",
        "Rust-owned Broadcom AMBA bus and per-core MMIO registry",
    ),
    rust_owned(
        "drivers/comedi",
        "src/drivers/comedi",
        "Rust-owned COMEDI DAQ subdevice and sample registry",
    ),
    rust_owned(
        "drivers/connector",
        "src/drivers/connector",
        "Rust-owned connector callback and message-delivery registry",
    ),
    rust_owned(
        "drivers/dca",
        "src/drivers/dca",
        "Rust-owned direct-cache-access provider and tag allocator",
    ),
    rust_owned(
        "drivers/fsi",
        "src/drivers/fsi",
        "Rust-owned FSI master/slave CFAM memory registry",
    ),
    rust_owned(
        "drivers/fwctl",
        "src/drivers/fwctl",
        "Rust-owned firmware-control device and secure channel registry",
    ),
    rust_owned(
        "drivers/gpib",
        "src/drivers/gpib",
        "Rust-owned IEEE-488 board and instrument registry",
    ),
    rust_owned(
        "drivers/greybus",
        "src/drivers/greybus",
        "Rust-owned Greybus interface, bundle, and CPort registry",
    ),
    rust_owned(
        "drivers/hv",
        "src/drivers/hv",
        "Rust-owned Hyper-V VMBus channel and device registry",
    ),
    rust_owned(
        "drivers/idle",
        "src/drivers/idle",
        "Rust-owned CPU idle driver/state accounting registry",
    ),
    rust_owned(
        "drivers/memstick",
        "src/drivers/memstick",
        "Rust-owned Memory Stick host/card block storage registry",
    ),
    rust_owned(
        "drivers/mcb",
        "src/drivers/mcb",
        "Rust-owned MEN Chameleon Bus and IP-core registry",
    ),
    rust_owned(
        "drivers/message",
        "src/drivers/message",
        "Rust-owned Fusion-MPT adapter, target, and reply queue model",
    ),
    rust_owned(
        "drivers/most",
        "src/drivers/most",
        "Rust-owned MOST interface, channel, and FIFO registry",
    ),
    rust_owned(
        "drivers/rapidio",
        "src/drivers/rapidio",
        "Rust-owned RapidIO mport/device routing registry",
    ),
    rust_owned(
        "drivers/siox",
        "src/drivers/siox",
        "Rust-owned SIOX master/device cycle registry",
    ),
    rust_owned(
        "drivers/ssb",
        "src/drivers/ssb",
        "Rust-owned Sonics Silicon Backplane bus/core registry",
    ),
];

/// Linux driver directories that still need Rust-owned mirror modules.
pub const PENDING_DRIVER_MIRRORS: &[LinuxMirrorEntry] = &[
    pending("drivers/accessibility", "src/drivers/accessibility"),
    pending("drivers/dibs", "src/drivers/dibs"),
    pending("drivers/dio", "src/drivers/dio"),
    pending("drivers/macintosh", "src/drivers/macintosh"),
    pending("drivers/nubus", "src/drivers/nubus"),
    pending("drivers/parisc", "src/drivers/parisc"),
    pending("drivers/ps3", "src/drivers/ps3"),
    pending("drivers/s390", "src/drivers/s390"),
    pending("drivers/sbus", "src/drivers/sbus"),
    pending("drivers/sh", "src/drivers/sh"),
    pending("drivers/staging", "src/drivers/staging"),
    pending("drivers/tc", "src/drivers/tc"),
    pending("drivers/zorro", "src/drivers/zorro"),
];

const fn rust_owned(
    linux_path: &'static str,
    rustos_path: &'static str,
    note: &'static str,
) -> LinuxMirrorEntry {
    LinuxMirrorEntry {
        linux_path,
        rustos_path,
        status: MirrorStatus::RustOwned,
        note,
    }
}

const fn pending(linux_path: &'static str, rustos_path: &'static str) -> LinuxMirrorEntry {
    LinuxMirrorEntry {
        linux_path,
        rustos_path,
        status: MirrorStatus::PendingRustOwned,
        note: "needs Rust-owned module before C shim fallback",
    }
}

/// Number of Linux driver directories still needing Rust-owned mirror work.
pub const fn pending_driver_mirror_count() -> usize {
    PENDING_DRIVER_MIRRORS.len()
}

/// Find known mirror metadata for a Linux driver source path.
pub fn driver_mirror_entry(linux_path: &str) -> Option<&'static LinuxMirrorEntry> {
    DRIVER_ALIASES
        .iter()
        .chain(RUST_OWNED_DRIVER_MIRRORS.iter())
        .chain(PENDING_DRIVER_MIRRORS.iter())
        .find(|entry| entry.linux_path == linux_path)
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!(
        "linux_mirror: {} aliases, {} rust-owned mirrors, {} pending mirrors",
        DRIVER_ALIASES.len(),
        RUST_OWNED_DRIVER_MIRRORS.len(),
        PENDING_DRIVER_MIRRORS.len()
    );
    Ok(())
}
