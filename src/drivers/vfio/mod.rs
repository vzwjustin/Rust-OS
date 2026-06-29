//! VFIO (Virtual Function I/O) subsystem
//!
//! Provides secure device passthrough for user-space driver and VM device assignment.
//! Mirrors Linux's `drivers/vfio/vfio.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// VFIO group status (Linux `enum vfio_group_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfioGroupStatus {
    Invalid,
    Viable,
    ContainerSet,
}

/// VFIO device type (Linux `enum vfio_device_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfioDeviceType {
    Pci,
    Platform,
    Amba,
    Cdx,
}

/// VFIO IOMMU type (Linux `enum vfio_iommu_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfioIommuType {
    Type1,
    Spapr,
    Noop,
}

/// VFIO region info (Linux `struct vfio_region_info`).
#[derive(Debug, Clone)]
pub struct VfioRegionInfo {
    pub index: u32,
    pub flags: u32,
    pub size: u64,
    pub offset: u64,
    pub cap_offset: u32,
}

/// VFIO IRQ info (Linux `struct vfio_irq_info`).
#[derive(Debug, Clone)]
pub struct VfioIrqInfo {
    pub index: u32,
    pub count: u32,
    pub flags: u32,
}

/// VFIO device operations (Linux `struct vfio_device_ops`).
pub struct VfioDeviceOps {
    pub open: fn(device_id: u32) -> Result<(), &'static str>,
    pub release: fn(device_id: u32) -> Result<(), &'static str>,
    pub read: fn(
        device_id: u32,
        buf: &mut [u8],
        count: usize,
        offset: u64,
    ) -> Result<usize, &'static str>,
    pub write:
        fn(device_id: u32, buf: &[u8], count: usize, offset: u64) -> Result<usize, &'static str>,
    pub ioctl: fn(device_id: u32, cmd: u32, arg: u64) -> Result<i32, &'static str>,
    pub mmap: fn(device_id: u32, offset: u64, size: u64) -> Result<u64, &'static str>,
    pub request: fn(device_id: u32, count: u32) -> Result<(), &'static str>,
}

/// VFIO device (Linux `struct vfio_device`).
pub struct VfioDevice {
    pub id: u32,
    pub name: String,
    pub dev_type: VfioDeviceType,
    pub group_id: u32,
    pub ops: VfioDeviceOps,
    pub regions: Vec<VfioRegionInfo>,
    pub irqs: Vec<VfioIrqInfo>,
    pub opened: bool,
}

/// VFIO group (Linux `struct vfio_group`).
pub struct VfioGroup {
    pub id: u32,
    pub status: VfioGroupStatus,
    pub device_ids: Vec<u32>,
    pub container_id: Option<u32>,
}

/// VFIO container (Linux `struct vfio_container`).
pub struct VfioContainer {
    pub id: u32,
    pub iommu_type: Option<VfioIommuType>,
    pub group_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CONTAINER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static GROUP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static VFIO_CONTAINERS: RwLock<BTreeMap<u32, VfioContainer>> = RwLock::new(BTreeMap::new());
static VFIO_GROUPS: RwLock<BTreeMap<u32, VfioGroup>> = RwLock::new(BTreeMap::new());
static VFIO_DEVICES: RwLock<BTreeMap<u32, VfioDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Create a VFIO container (Linux `VFIO_GET_API_VERSION` + container open).
pub fn create_container() -> Result<u32, &'static str> {
    let id = CONTAINER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let container = VfioContainer {
        id,
        iommu_type: None,
        group_ids: Vec::new(),
    };
    VFIO_CONTAINERS.write().insert(id, container);
    Ok(id)
}

/// Set the IOMMU type for a container (Linux `VFIO_SET_IOMMU`).
pub fn set_iommu_type(container_id: u32, iommu_type: VfioIommuType) -> Result<(), &'static str> {
    let mut containers = VFIO_CONTAINERS.write();
    let container = containers
        .get_mut(&container_id)
        .ok_or("VFIO container not found")?;
    container.iommu_type = Some(iommu_type);
    Ok(())
}

/// Create a VFIO group.
pub fn create_group() -> Result<u32, &'static str> {
    let id = GROUP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let group = VfioGroup {
        id,
        status: VfioGroupStatus::Viable,
        device_ids: Vec::new(),
        container_id: None,
    };
    VFIO_GROUPS.write().insert(id, group);
    Ok(id)
}

/// Set a group's container (Linux `VFIO_GROUP_SET_CONTAINER`).
pub fn set_group_container(group_id: u32, container_id: u32) -> Result<(), &'static str> {
    {
        let mut groups = VFIO_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("VFIO group not found")?;
        group.container_id = Some(container_id);
        group.status = VfioGroupStatus::ContainerSet;
    }
    {
        let mut containers = VFIO_CONTAINERS.write();
        let container = containers
            .get_mut(&container_id)
            .ok_or("VFIO container not found")?;
        container.group_ids.push(group_id);
    }
    Ok(())
}

/// Unset a group's container (Linux `VFIO_GROUP_UNSET_CONTAINER`).
pub fn unset_group_container(group_id: u32) -> Result<(), &'static str> {
    let container_id = {
        let mut groups = VFIO_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("VFIO group not found")?;
        group.container_id.take().ok_or("Group has no container")?
    };
    {
        let mut groups = VFIO_GROUPS.write();
        let group = groups.get_mut(&group_id).ok_or("VFIO group not found")?;
        group.status = VfioGroupStatus::Viable;
    }
    let mut containers = VFIO_CONTAINERS.write();
    if let Some(container) = containers.get_mut(&container_id) {
        container.group_ids.retain(|&g| g != group_id);
    }
    Ok(())
}

/// Register a VFIO device in a group.
pub fn register_device(
    name: &str,
    dev_type: VfioDeviceType,
    group_id: u32,
    ops: VfioDeviceOps,
    regions: Vec<VfioRegionInfo>,
    irqs: Vec<VfioIrqInfo>,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = VfioDevice {
        id,
        name: String::from(name),
        dev_type,
        group_id,
        ops,
        regions,
        irqs,
        opened: false,
    };
    VFIO_DEVICES.write().insert(id, dev);

    let mut groups = VFIO_GROUPS.write();
    let group = groups.get_mut(&group_id).ok_or("VFIO group not found")?;
    group.device_ids.push(id);
    Ok(id)
}

/// Open a VFIO device (Linux `VFIO_GROUP_GET_DEVICE_FD`).
pub fn open_device(device_id: u32) -> Result<(), &'static str> {
    let open_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if dev.opened {
            return Err("VFIO device already open");
        }
        dev.ops.open
    };
    (open_fn)(device_id)?;

    let mut devices = VFIO_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.opened = true;
    }
    Ok(())
}

/// Release (close) a VFIO device.
pub fn release_device(device_id: u32) -> Result<(), &'static str> {
    let release_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if !dev.opened {
            return Err("VFIO device not open");
        }
        dev.ops.release
    };
    (release_fn)(device_id)?;

    let mut devices = VFIO_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.opened = false;
    }
    Ok(())
}

/// Read from a VFIO device region (Linux `VFIO_DEVICE_REGION_READ`).
pub fn device_read(
    device_id: u32,
    buf: &mut [u8],
    count: usize,
    offset: u64,
) -> Result<usize, &'static str> {
    let read_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if !dev.opened {
            return Err("VFIO device not open");
        }
        dev.ops.read
    };
    (read_fn)(device_id, buf, count, offset)
}

/// Write to a VFIO device region (Linux `VFIO_DEVICE_REGION_WRITE`).
pub fn device_write(
    device_id: u32,
    buf: &[u8],
    count: usize,
    offset: u64,
) -> Result<usize, &'static str> {
    let write_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if !dev.opened {
            return Err("VFIO device not open");
        }
        dev.ops.write
    };
    (write_fn)(device_id, buf, count, offset)
}

/// Perform an ioctl on a VFIO device.
pub fn device_ioctl(device_id: u32, cmd: u32, arg: u64) -> Result<i32, &'static str> {
    let ioctl_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if !dev.opened {
            return Err("VFIO device not open");
        }
        dev.ops.ioctl
    };
    (ioctl_fn)(device_id, cmd, arg)
}

/// MMap a VFIO device region.
pub fn device_mmap(device_id: u32, offset: u64, size: u64) -> Result<u64, &'static str> {
    let mmap_fn = {
        let devices = VFIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
        if !dev.opened {
            return Err("VFIO device not open");
        }
        dev.ops.mmap
    };
    (mmap_fn)(device_id, offset, size)
}

/// Get device region info.
pub fn get_region_info(device_id: u32, index: u32) -> Result<VfioRegionInfo, &'static str> {
    let devices = VFIO_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
    dev.regions
        .iter()
        .find(|r| r.index == index)
        .cloned()
        .ok_or("Region not found")
}

/// Get device IRQ info.
pub fn get_irq_info(device_id: u32, index: u32) -> Result<VfioIrqInfo, &'static str> {
    let devices = VFIO_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("VFIO device not found")?;
    dev.irqs
        .iter()
        .find(|i| i.index == index)
        .cloned()
        .ok_or("IRQ not found")
}

/// List all VFIO groups.
pub fn list_groups() -> Vec<(u32, VfioGroupStatus, usize)> {
    VFIO_GROUPS
        .read()
        .iter()
        .map(|(id, g)| (*id, g.status, g.device_ids.len()))
        .collect()
}

/// List devices in a group.
pub fn list_group_devices(
    group_id: u32,
) -> Result<Vec<(u32, String, VfioDeviceType)>, &'static str> {
    let groups = VFIO_GROUPS.read();
    let group = groups.get(&group_id).ok_or("VFIO group not found")?;
    let devices = VFIO_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &group.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.name.clone(), dev.dev_type));
        }
    }
    Ok(result)
}

/// Count registered devices.
pub fn device_count() -> usize {
    VFIO_DEVICES.read().len()
}

// ── Software VFIO ───────────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read(
    _dev_id: u32,
    buf: &mut [u8],
    count: usize,
    _offset: u64,
) -> Result<usize, &'static str> {
    let n = core::cmp::min(buf.len(), count);
    for b in buf[..n].iter_mut() {
        *b = 0;
    }
    Ok(n)
}
fn sw_write(_dev_id: u32, _buf: &[u8], count: usize, _offset: u64) -> Result<usize, &'static str> {
    Ok(count)
}
fn sw_ioctl(_dev_id: u32, _cmd: u32, _arg: u64) -> Result<i32, &'static str> {
    Ok(0)
}
fn sw_mmap(_dev_id: u32, _offset: u64, _size: u64) -> Result<u64, &'static str> {
    Ok(0)
}
fn sw_request(_dev_id: u32, _count: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software VFIO device ops.
pub fn software_vfio_ops() -> VfioDeviceOps {
    VfioDeviceOps {
        open: sw_open,
        release: sw_release,
        read: sw_read,
        write: sw_write,
        ioctl: sw_ioctl,
        mmap: sw_mmap,
        request: sw_request,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Create a container
    let container_id = create_container()?;
    set_iommu_type(container_id, VfioIommuType::Type1)?;

    // Create a group and set its container
    let group_id = create_group()?;
    set_group_container(group_id, container_id)?;

    // Register a software VFIO PCI device
    let ops = software_vfio_ops();
    let mut regions = Vec::new();
    regions.push(VfioRegionInfo {
        index: 0,
        flags: 0x3, // read + write
        size: 4096,
        offset: 0,
        cap_offset: 0,
    });
    let mut irqs = Vec::new();
    irqs.push(VfioIrqInfo {
        index: 0,
        count: 1,
        flags: 0,
    });
    register_device(
        "sw-vfio-pci0",
        VfioDeviceType::Pci,
        group_id,
        ops,
        regions,
        irqs,
    )?;

    Ok(())
}
