//! Compute accelerator subsystem (mirrors Linux `drivers/accel/`)
//!
//! Registers compute accelerator devices (the `accel` char-device class used
//! for AI/ML and DSP offload engines) and dispatches opaque work submissions
//! to their driver ops. Includes a software accelerator for platform use.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelKind {
    Gpu,
    Npu,
    Dsp,
    Fpga,
    Software,
}

pub struct AccelOps {
    /// Submit a command buffer; returns bytes produced into the result vec.
    pub submit: fn(payload: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct AccelDevice {
    id: u32,
    name: String,
    kind: AccelKind,
    vendor_id: u16,
    submissions: u64,
    ops: AccelOps,
}

// ── Software accelerator (identity/echo engine) ───────────────────────────

fn software_submit(payload: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str> {
    out.clear();
    out.extend_from_slice(payload);
    Ok(())
}

const SOFTWARE_OPS: AccelOps = AccelOps {
    submit: software_submit,
    get_name: || "accel-soft",
};

// ── Registry ──────────────────────────────────────────────────────────────

static ACCELS: RwLock<BTreeMap<u32, AccelDevice>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_device(
    name: &str,
    kind: AccelKind,
    vendor_id: u16,
    ops: AccelOps,
) -> Result<u32, &'static str> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    ACCELS.write().insert(
        id,
        AccelDevice {
            id,
            name: String::from(name),
            kind,
            vendor_id,
            submissions: 0,
            ops,
        },
    );
    Ok(id)
}

pub fn submit(id: u32, payload: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str> {
    let mut devs = ACCELS.write();
    let dev = devs.get_mut(&id).ok_or("accel: device not found")?;
    (dev.ops.submit)(payload, out)?;
    dev.submissions += 1;
    Ok(())
}

pub fn device_kind(id: u32) -> Option<AccelKind> {
    ACCELS.read().get(&id).map(|d| d.kind)
}

pub fn device_count() -> usize {
    ACCELS.read().len()
}

/// Initialize the accelerator subsystem with a software engine.
pub fn init() -> Result<(), &'static str> {
    if !ACCELS.read().is_empty() {
        return Ok(());
    }
    register_device("accel0", AccelKind::Software, 0x1af4, SOFTWARE_OPS)?;
    crate::serial_println!("accel: {} accelerator(s) registered", device_count());
    Ok(())
}
