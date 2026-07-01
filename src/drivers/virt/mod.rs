//! Virt (virtualization) driver subsystem
//!
//! Provides virtualization detection and management framework.
//! Mirrors Linux's `drivers/virt/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Hypervisor type (Linux `enum hypervisor_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypervisorType {
    None,
    Xen,
    Kvm,
    Vmware,
    HyperV,
    Qemu,
    Bhyve,
    AppleVz,
}

/// Virtual machine info.
pub struct VmInfo {
    pub id: u32,
    pub hypervisor: HypervisorType,
    pub name: String,
    pub vcpu_count: u32,
    pub memory_mb: u64,
    pub features: u32,
}

/// Virtualization driver (Linux `struct virt_driver`).
pub struct VirtDriver {
    pub id: u32,
    pub name: String,
    pub hypervisor: HypervisorType,
    pub detect: fn() -> bool,
    pub init: fn() -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static VM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static VM_INFO: RwLock<Option<VmInfo>> = RwLock::new(None);
static VIRT_DRIVERS: RwLock<BTreeMap<u32, VirtDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Detect the hypervisor type using CPUID.
pub fn detect_hypervisor() -> HypervisorType {
    let ebx_val: u64;
    let ecx: u32;
    let edx: u32;
    let eax: u32;

    unsafe {
        core::arch::asm!(
            "xchg rbx, {save}",
            "cpuid",
            "xchg rbx, {save}",
            save = inout(reg) 0u64 => ebx_val,
            inout("eax") 0x40000000u32 => eax,
            out("ecx") ecx,
            out("edx") edx,
        );
    }
    let ebx: u32 = ebx_val as u32;

    if eax < 0x40000000 {
        return HypervisorType::None;
    }

    let signature = [ebx, ecx, edx];
    let sig_bytes = signature
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();

    if sig_bytes.starts_with(b"XenV") {
        HypervisorType::Xen
    } else if sig_bytes.starts_with(b"KVMK") {
        HypervisorType::Kvm
    } else if sig_bytes.starts_with(b"VMwa") {
        HypervisorType::Vmware
    } else if sig_bytes.starts_with(b"Micr") {
        HypervisorType::HyperV
    } else if sig_bytes.starts_with(b"TCGTCG") || sig_bytes.starts_with(b"QEMU") {
        HypervisorType::Qemu
    } else {
        HypervisorType::None
    }
}

/// Register VM info.
pub fn register_vm_info(
    hypervisor: HypervisorType,
    name: &str,
    vcpu_count: u32,
    memory_mb: u64,
    features: u32,
) -> Result<u32, &'static str> {
    let id = VM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let info = VmInfo {
        id,
        hypervisor,
        name: String::from(name),
        vcpu_count,
        memory_mb,
        features,
    };
    *VM_INFO.write() = Some(info);
    Ok(id)
}

/// Register a virtualization driver.
pub fn register_driver(
    name: &str,
    hypervisor: HypervisorType,
    detect: fn() -> bool,
    init: fn() -> Result<(), &'static str>,
) -> Result<u32, &'static str> {
    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv = VirtDriver {
        id,
        name: String::from(name),
        hypervisor,
        detect,
        init,
    };
    VIRT_DRIVERS.write().insert(id, drv);
    Ok(id)
}

/// Get VM info.
pub fn get_vm_info() -> Option<(HypervisorType, String, u32, u64)> {
    VM_INFO.read().as_ref().map(|info| {
        (
            info.hypervisor,
            info.name.clone(),
            info.vcpu_count,
            info.memory_mb,
        )
    })
}

/// List all virtualization drivers.
pub fn list_drivers() -> Vec<(u32, String, HypervisorType)> {
    VIRT_DRIVERS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.hypervisor))
        .collect()
}

/// Count drivers.
pub fn driver_count() -> usize {
    VIRT_DRIVERS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if VM_INFO.read().is_some() {
        return Ok(());
    }

    let hv = detect_hypervisor();
    let hv_name = match hv {
        HypervisorType::None => "bare-metal",
        HypervisorType::Xen => "Xen",
        HypervisorType::Kvm => "KVM",
        HypervisorType::Vmware => "VMware",
        HypervisorType::HyperV => "Hyper-V",
        HypervisorType::Qemu => "QEMU",
        HypervisorType::Bhyve => "bhyve",
        HypervisorType::AppleVz => "Apple Virtualization",
    };

    register_vm_info(hv, hv_name, 1, 256, 0)?;

    crate::serial_println!("virt: hypervisor detected: {}", hv_name);
    Ok(())
}
