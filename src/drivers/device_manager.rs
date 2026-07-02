//! Device-manager `/dev` population pass.
//!
//! Bridges the unified device model (`crate::drivers::base`, which subsystem
//! drivers register into via `register_device_simple()`/`register_device()`)
//! and `crate::fs::devfs` (the `DevFs` actually mounted at `/dev` for
//! userspace `open()`/`read()` syscalls, see `src/fs/mod.rs`).
//!
//! `crate::fs::devfs::register_block_devices()` already covers block/scsi/nvme
//! devices by querying `crate::block_io` directly. This module fills the
//! remaining gap: devices registered on the `"input"` bus (e.g. the PS/2
//! keyboard driver in `crate::drivers::input::init()`) have no devfs node of
//! their own, so nothing under `/dev` lets userspace (evdev-consuming
//! programs launched by `/init`, libinput probes, etc.) read them. This pass
//! walks the device-model registry and creates one `/dev/inputN` character
//! device per registered input-bus device that doesn't already have a node,
//! wired to real event data via `crate::fs::devfs`'s `DeviceType::Input`
//! read path (which pulls from `crate::drivers::input_manager::get_event()`).

/// Populate `/dev` with nodes for devices in the unified device model
/// (`crate::drivers::base`) that `crate::fs::devfs::register_block_devices()`
/// doesn't already cover.
///
/// Must run after `crate::fs::devfs` has been mounted (`crate::fs::init()`)
/// and after the per-subsystem `base::register_device_simple()` calls earlier
/// in boot (e.g. `crate::drivers::input::init()`) have populated the device
/// registry, so every input device discovered during hardware bring-up gets
/// a node. Idempotent: re-running skips devices that already have a devfs
/// node under their assigned name.
pub fn populate_dev_nodes() {
    let mut input_index: u32 = 0;
    for device in crate::drivers::base::all_devices() {
        if device.bus != "input" {
            continue;
        }

        let dev_name = alloc::format!("input{}", input_index);
        input_index += 1;

        // Major 13 mirrors Linux's INPUT_MAJOR; minor is the per-device index.
        match crate::fs::devfs::create_device_node(
            &dev_name,
            crate::fs::devfs::DeviceType::Input,
            13,
            input_index - 1,
            crate::fs::FilePermissions::from_octal(0o660),
        ) {
            Ok(()) => {
                crate::serial_println!(
                    "[device_manager] registered /dev/{} (input, base device id={}, name={:?})",
                    dev_name,
                    device.id,
                    device.name
                );
            }
            Err(crate::fs::FsError::AlreadyExists) => {
                // Already populated by an earlier boot pass; not an error.
            }
            Err(e) => {
                crate::serial_println!(
                    "[device_manager] failed to register /dev/{} for base device id={}: {:?}",
                    dev_name,
                    device.id,
                    e
                );
            }
        }
    }

    crate::serial_println!(
        "[device_manager] /dev population pass complete ({} input node(s))",
        input_index
    );
}
