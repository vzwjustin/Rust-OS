//! RustOS kernel-side installer — partition, format, copy, configure.

pub mod config;
pub mod copy;
pub mod disk;
pub mod format;
pub mod partition;
pub mod plan;
pub mod ui;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub use plan::InstallPlan;

/// Installer runtime status for /proc/rustos/installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerStatus {
    Idle,
    Running,
    Complete,
    Error,
}

impl InstallerStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallerStatus::Idle => "idle",
            InstallerStatus::Running => "running",
            InstallerStatus::Complete => "complete",
            InstallerStatus::Error => "error",
        }
    }
}

struct InstallerState {
    status: InstallerStatus,
    progress: u8,
    log: Vec<String>,
    install_mode: bool,
    last_error: String,
}

impl InstallerState {
    fn new() -> Self {
        Self {
            status: InstallerStatus::Idle,
            progress: 0,
            log: Vec::new(),
            install_mode: false,
            last_error: String::new(),
        }
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() >= 64 {
            self.log.remove(0);
        }
        self.log.push(line);
        crate::serial_println!(
            "installer: {}",
            self.log.last().map(|s| s.as_str()).unwrap_or("")
        );
    }

    fn set_progress(&mut self, pct: u8, msg: &str) {
        self.progress = pct.min(100);
        self.push_log(format!("[{}%] {}", self.progress, msg));
        let _ =
            crate::vfs::procfs::update_installer_status(self.status.as_str(), self.progress as u32);
    }
}

static INSTALLER: spin::Mutex<Option<InstallerState>> = spin::Mutex::new(None);

fn with_installer<F, R>(f: F) -> R
where
    F: FnOnce(&mut InstallerState) -> R,
{
    let mut guard = INSTALLER.lock();
    if guard.is_none() {
        *guard = Some(InstallerState::new());
    }
    f(guard.as_mut().unwrap())
}

/// Initialize installer subsystem (storage FS scan + procfs).
pub fn init() {
    crate::drivers::storage::filesystem_interface::init_filesystem_interface();
    let _ = crate::drivers::storage::filesystem_interface::scan_all_storage_filesystems();

    with_installer(|state| {
        state.status = InstallerStatus::Idle;
        state.push_log(String::from("Installer initialized"));
    });

    let _ = crate::vfs::procfs::update_installer_status("idle", 0);
}

/// Whether the boot menu selected install mode.
pub fn is_live_mode() -> bool {
    if crate::boot_ui::boot_config().live_mode {
        return true;
    }
    crate::vfs::procfs::installer_mode() == "live"
}

pub fn is_install_mode() -> bool {
    if crate::boot_ui::boot_config().install_mode {
        return true;
    }
    with_installer(|state| state.install_mode)
}

/// Mark install mode active (boot menu selection).
pub fn set_install_mode(enabled: bool) {
    with_installer(|state| state.install_mode = enabled);
}

/// Current status string for procfs.
pub fn installer_status() -> InstallerStatus {
    with_installer(|state| state.status)
}

/// Current progress 0-100.
pub fn installer_progress() -> u8 {
    with_installer(|state| state.progress)
}

/// Log lines for procfs.
pub fn installer_log() -> Vec<String> {
    with_installer(|state| state.log.clone())
}

/// Procfs file body.
pub fn installer_proc_content() -> String {
    with_installer(|state| {
        let mut out = format!(
            "status={}\nprogress={}\n",
            state.status.as_str(),
            state.progress
        );
        if !state.last_error.is_empty() {
            out.push_str(&format!("error={}\n", state.last_error));
        }
        out.push_str("log_begin\n");
        for line in state.log.iter() {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("log_end\n");
        out
    })
}

fn set_status(status: InstallerStatus) {
    with_installer(|state| {
        state.status = status;
        let _ = crate::vfs::procfs::update_installer_status(status.as_str(), state.progress as u32);
    });
}

fn set_error(msg: &str) {
    with_installer(|state| {
        state.status = InstallerStatus::Error;
        state.last_error = String::from(msg);
        state.push_log(format!("ERROR: {}", msg));
        let _ = crate::vfs::procfs::update_installer_status("error", state.progress as u32);
    });
}

/// Interactive wizard; returns finalized plan or error.
pub fn run_wizard() -> Result<InstallPlan, String> {
    set_status(InstallerStatus::Running);
    with_installer(|state| state.set_progress(0, "Starting installer wizard"));

    let mut plan = InstallPlan::default();
    let disks = disk::enumerate_disks();
    if disks.is_empty() {
        let msg = "no storage devices found";
        set_error(msg);
        return Err(String::from(msg));
    }
    if let Some(id) = disk::default_target_disk(&disks) {
        plan.target_device_id = id;
    } else {
        plan.target_device_id = disks[0].id;
    }

    let use_graphical = crate::graphics::framebuffer::framebuffer().is_some();
    let result = if use_graphical {
        ui::run_graphical_wizard(plan)
    } else {
        ui::run_text_wizard(plan)
    };

    result
        .map(|plan| {
            with_installer(|state| state.set_progress(10, "Install plan confirmed"));
            plan
        })
        .map_err(|e| {
            set_error(&e);
            e
        })
}

/// Execute partitioning, formatting, copy, and configuration.
pub fn apply_plan(plan: &InstallPlan) -> Result<(), String> {
    set_status(InstallerStatus::Running);
    with_installer(|state| state.set_progress(5, "Creating partition layout"));

    let layout = partition::create_partition_layout(plan.target_device_id, plan.erase_disk)
        .map_err(|e| format!("partition failed: {}", e))?;

    with_installer(|state| state.set_progress(20, "Formatting EFI partition (vfat)"));
    format::format_vfat(
        layout.device_id,
        layout.efi_start_sector,
        layout.efi_sector_count,
        "EFI",
    )
    .map_err(|e| format!("vfat format failed: {}", e))?;

    with_installer(|state| state.set_progress(35, "Formatting root partition (ext4)"));
    let mut volume = format::format_ext4(
        layout.device_id,
        layout.root_start_sector,
        layout.root_sector_count,
        "RustOS",
    )
    .map_err(|e| format!("ext4 format failed: {}", e))?;

    with_installer(|state| state.set_progress(50, "Copying system files"));
    let copied =
        copy::copy_rootfs_to_partition(&mut volume).map_err(|e| format!("copy failed: {}", e))?;
    with_installer(|state| {
        state.set_progress(75, &format!("Copied {} files", copied));
    });

    with_installer(|state| state.set_progress(85, "Writing configuration"));
    config::write_install_config(&mut volume, plan, &layout)
        .map_err(|e| format!("config failed: {}", e))?;

    if plan.include_swap {
        if let (Some(start), Some(count)) = (layout.swap_start_sector, layout.swap_sector_count) {
            with_installer(|state| state.set_progress(92, "Initializing swap area"));
            wipe_swap(layout.device_id, start, count);
        }
    }

    with_installer(|state| state.set_progress(100, "Install plan applied"));
    Ok(())
}

fn wipe_swap(device_id: u32, start: u64, sectors: u64) {
    let sector = [0u8; 512];
    let max = start.saturating_add(sectors.min(2048));
    for lba in start..max {
        let _ = crate::drivers::storage::write_storage_sectors(device_id, lba, &sector);
    }
}

/// Reboot after successful install (falls back to clearing install mode).
pub fn finish_install_and_reboot() -> ! {
    crate::serial_println!("installer: rebooting into installed system");
    set_install_mode(false);
    let _ = crate::linux_compat::sysinfo_ops::reboot(
        0xfee1dead_u32 as i32,
        672274793,
        0x01234567,
        core::ptr::null_mut(),
    );
    loop {
        crate::serial_println!("installer: reset requested — power cycle or reboot via firmware");
        for _ in 0..50_000_000 {
            core::hint::spin_loop();
        }
    }
}
