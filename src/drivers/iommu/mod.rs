//! IOMMU (I/O Memory Management Unit) subsystem
//!
//! Provides DMA address translation, isolation, and protection for devices.
//! Mirrors Linux's `drivers/iommu/iommu.c` framework.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IOMMU domain type (Linux `enum iommu_domain_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuDomainType {
    Unmanaged,
    _dma,
    Identity,
    Blocked,
    Sva,
}

/// IOMMU page size capabilities (bitmap of supported page sizes).
pub type PageSizeCap = u64;

/// IOMMU fault type (Linux `enum iommu_fault_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuFaultType {
    Unknown,
    BadRequest,
    TranslationFault,
    PermissionFault,
    AccessFault,
    HardwareFailure,
}

/// IOMMU fault event (Linux `struct iommu_fault`).
#[derive(Debug, Clone)]
pub struct IommuFaultEvent {
    pub fault_type: IommuFaultType,
    pub device_id: u32,
    pub iova: u64,
    pub flags: u32,
}

/// IOMMU domain (Linux `struct iommu_domain`).
pub struct IommuDomain {
    pub domain_type: IommuDomainType,
    pub pgsize_bitmap: PageSizeCap,
    pub geometry_aperture_start: u64,
    pub geometry_aperture_end: u64,
    pub iova_cookie: Option<u64>,
    pub attached_groups: Vec<u32>,
}

/// IOMMU group (Linux `struct iommu_group`).
pub struct IommuGroup {
    pub id: u32,
    pub name: String,
    pub devices: Vec<u32>,
    pub domain: Option<u32>,
}

/// IOMMU device operations (Linux `struct iommu_ops`).
pub struct IommuOps {
    pub domain_alloc: fn(domain_type: IommuDomainType) -> Result<u32, &'static str>,
    pub domain_free: fn(domain_id: u32) -> Result<(), &'static str>,
    pub attach_dev: fn(domain_id: u32, device_id: u32) -> Result<(), &'static str>,
    pub detach_dev: fn(domain_id: u32, device_id: u32) -> Result<(), &'static str>,
    pub map: fn(
        domain_id: u32,
        iova: u64,
        phys: u64,
        size: u64,
        prot: IommuProt,
    ) -> Result<(), &'static str>,
    pub unmap: fn(domain_id: u32, iova: u64, size: u64) -> Result<u64, &'static str>,
    pub iova_to_phys: fn(domain_id: u32, iova: u64) -> Result<u64, &'static str>,
    pub probe_device: fn(device_id: u32) -> Result<(), &'static str>,
    pub release_device: fn(device_id: u32) -> Result<(), &'static str>,
    pub page_response: fn(domain_id: u32, event: &IommuFaultEvent) -> Result<(), &'static str>,
}

/// IOMMU protection flags (Linux `enum iommu_prot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IommuProt(pub u32);

impl IommuProt {
    pub const NONE: Self = IommuProt(0);
    pub const READ: Self = IommuProt(1);
    pub const WRITE: Self = IommuProt(2);
    pub const PRIV: Self = IommuProt(4);
    pub const EXEC: Self = IommuProt(8);
    pub const MMIO: Self = IommuProt(16);

    pub fn has_read(&self) -> bool {
        self.0 & Self::READ.0 != 0
    }
    pub fn has_write(&self) -> bool {
        self.0 & Self::WRITE.0 != 0
    }
    pub fn has_exec(&self) -> bool {
        self.0 & Self::EXEC.0 != 0
    }
}

/// IOMMU controller instance.
pub struct IommuController {
    pub name: String,
    pub ops: IommuOps,
    pub supported_page_sizes: PageSizeCap,
    pub num_domains: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static GROUP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DOMAIN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IOMMU_CONTROLLERS: RwLock<BTreeMap<u32, IommuController>> = RwLock::new(BTreeMap::new());
static IOMMU_GROUPS: RwLock<BTreeMap<u32, IommuGroup>> = RwLock::new(BTreeMap::new());
static IOMMU_DOMAINS: RwLock<BTreeMap<u32, IommuDomain>> = RwLock::new(BTreeMap::new());
static DEVICE_TO_GROUP: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());
static CONTROLLER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an IOMMU controller.
pub fn register_controller(
    name: &str,
    ops: IommuOps,
    supported_page_sizes: PageSizeCap,
    num_domains: u32,
) -> Result<u32, &'static str> {
    let id = CONTROLLER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = IommuController {
        name: String::from(name),
        ops,
        supported_page_sizes,
        num_domains,
    };
    IOMMU_CONTROLLERS.write().insert(id, ctrl);
    Ok(id)
}

/// Allocate an IOMMU domain.
pub fn alloc_domain(domain_type: IommuDomainType, controller_id: u32) -> Result<u32, &'static str> {
    let domain_id = DOMAIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let pgsize_bitmap = {
        let ctrls = IOMMU_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&controller_id)
            .ok_or("IOMMU controller not found")?;
        ctrl.supported_page_sizes
    };

    let domain = IommuDomain {
        domain_type,
        pgsize_bitmap,
        geometry_aperture_start: 0,
        geometry_aperture_end: 0xFFFF_FFFF_FFFF_F000,
        iova_cookie: None,
        attached_groups: Vec::new(),
    };

    IOMMU_DOMAINS.write().insert(domain_id, domain);
    Ok(domain_id)
}

/// Free an IOMMU domain.
pub fn free_domain(domain_id: u32) -> Result<(), &'static str> {
    let mut domains = IOMMU_DOMAINS.write();
    if domains.remove(&domain_id).is_none() {
        return Err("IOMMU domain not found");
    }
    Ok(())
}

/// Create an IOMMU group.
pub fn alloc_group(name: &str) -> Result<u32, &'static str> {
    let id = GROUP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let group = IommuGroup {
        id,
        name: String::from(name),
        devices: Vec::new(),
        domain: None,
    };
    IOMMU_GROUPS.write().insert(id, group);
    Ok(id)
}

/// Add a device to an IOMMU group.
pub fn add_device_to_group(group_id: u32, device_id: u32) -> Result<(), &'static str> {
    {
        let mut groups = IOMMU_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("IOMMU group not found")?;
        group.devices.push(device_id);
    }
    DEVICE_TO_GROUP.write().insert(device_id, group_id);
    Ok(())
}

/// Remove a device from its IOMMU group.
pub fn remove_device_from_group(device_id: u32) -> Result<(), &'static str> {
    let group_id = {
        let mut d2g = DEVICE_TO_GROUP.write();
        d2g.remove(&device_id).ok_or("Device not in any group")?
    };
    let mut groups = IOMMU_GROUPS.write();
    let group = groups.get_mut(&group_id).ok_or("IOMMU group not found")?;
    group.devices.retain(|&d| d != device_id);
    Ok(())
}

/// Attach an IOMMU group to a domain.
pub fn attach_group_to_domain(domain_id: u32, group_id: u32) -> Result<(), &'static str> {
    {
        let mut groups = IOMMU_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("IOMMU group not found")?;
        group.domain = Some(domain_id);
    }
    {
        let mut domains = IOMMU_DOMAINS.write();
        let domain = domains
            .get_mut(&domain_id)
            .ok_or("IOMMU domain not found")?;
        if !domain.attached_groups.contains(&group_id) {
            domain.attached_groups.push(group_id);
        }
    }
    Ok(())
}

/// Detach an IOMMU group from its domain.
pub fn detach_group_from_domain(group_id: u32) -> Result<(), &'static str> {
    let domain_id = {
        let mut groups = IOMMU_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("IOMMU group not found")?;
        group
            .domain
            .take()
            .ok_or("Group not attached to any domain")?
    };
    let mut domains = IOMMU_DOMAINS.write();
    let domain = domains
        .get_mut(&domain_id)
        .ok_or("IOMMU domain not found")?;
    domain.attached_groups.retain(|&g| g != group_id);
    Ok(())
}

/// Map a physical address to an IOVA in a domain.
pub fn map(
    domain_id: u32,
    iova: u64,
    phys: u64,
    size: u64,
    prot: IommuProt,
) -> Result<(), &'static str> {
    let map_fn = {
        let ctrls = IOMMU_CONTROLLERS.read();
        let ctrl = ctrls
            .iter()
            .next()
            .ok_or("No IOMMU controller registered")?;
        ctrl.1.ops.map
    };
    (map_fn)(domain_id, iova, phys, size, prot)
}

/// Unmap an IOVA range from a domain.
pub fn unmap(domain_id: u32, iova: u64, size: u64) -> Result<u64, &'static str> {
    let unmap_fn = {
        let ctrls = IOMMU_CONTROLLERS.read();
        let ctrl = ctrls
            .iter()
            .next()
            .ok_or("No IOMMU controller registered")?;
        ctrl.1.ops.unmap
    };
    (unmap_fn)(domain_id, iova, size)
}

/// Translate an IOVA to a physical address.
pub fn iova_to_phys(domain_id: u32, iova: u64) -> Result<u64, &'static str> {
    let xlate_fn = {
        let ctrls = IOMMU_CONTROLLERS.read();
        let ctrl = ctrls
            .iter()
            .next()
            .ok_or("No IOMMU controller registered")?;
        ctrl.1.ops.iova_to_phys
    };
    (xlate_fn)(domain_id, iova)
}

/// Report a page response for a fault.
pub fn page_response(domain_id: u32, event: &IommuFaultEvent) -> Result<(), &'static str> {
    let pr_fn = {
        let ctrls = IOMMU_CONTROLLERS.read();
        let ctrl = ctrls
            .iter()
            .next()
            .ok_or("No IOMMU controller registered")?;
        ctrl.1.ops.page_response
    };
    (pr_fn)(domain_id, event)
}

/// Get the group ID for a device.
pub fn get_device_group(device_id: u32) -> Option<u32> {
    DEVICE_TO_GROUP.read().get(&device_id).copied()
}

/// List all registered IOMMU controllers.
pub fn list_controllers() -> Vec<(u32, String)> {
    IOMMU_CONTROLLERS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone()))
        .collect()
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    IOMMU_CONTROLLERS.read().len()
}

// ── Software IOMMU unsupported backend ──────────────────────────────────

fn sw_domain_alloc(_dt: IommuDomainType) -> Result<u32, &'static str> {
    Err("software IOMMU not available")
}

fn sw_domain_free(_domain_id: u32) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_attach_dev(_domain_id: u32, _device_id: u32) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_detach_dev(_domain_id: u32, _device_id: u32) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_map(
    _domain_id: u32,
    _iova: u64,
    _phys: u64,
    _size: u64,
    _prot: IommuProt,
) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_unmap(_domain_id: u32, _iova: u64, _size: u64) -> Result<u64, &'static str> {
    Err("software IOMMU not available")
}

fn sw_iova_to_phys(_domain_id: u32, _iova: u64) -> Result<u64, &'static str> {
    Err("software IOMMU not available")
}

fn sw_probe_device(_device_id: u32) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_release_device(_device_id: u32) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

fn sw_page_response(_domain_id: u32, _event: &IommuFaultEvent) -> Result<(), &'static str> {
    Err("software IOMMU not available")
}

pub fn software_iommu_ops() -> IommuOps {
    IommuOps {
        domain_alloc: sw_domain_alloc,
        domain_free: sw_domain_free,
        attach_dev: sw_attach_dev,
        detach_dev: sw_detach_dev,
        map: sw_map,
        unmap: sw_unmap,
        iova_to_phys: sw_iova_to_phys,
        probe_device: sw_probe_device,
        release_device: sw_release_device,
        page_response: sw_page_response,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("iommu: subsystem ready");
    Ok(())
}
