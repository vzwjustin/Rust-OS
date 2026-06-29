//! IOMMUFD (IOMMU userspace API) subsystem
//!
//! Provides userspace IOMMU interface for device passthrough and IOASID management.
//! Mirrors Linux's `drivers/iommu/iommufd/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IOMMUFD file context (Linux `struct iommufd_ctx`).
pub struct IommufdCtx {
    pub id: u32,
    pub ioas_ids: Vec<u32>,
    pub hwpt_ids: Vec<u32>,
    pub device_ids: Vec<u32>,
}

/// IOAS (IO Address Space) (Linux `struct iommufd_ioas`).
pub struct IommufdIoas {
    pub id: u32,
    pub ctx_id: u32,
    pub name: String,
    pub mappings: BTreeMap<u64, IoasMapping>,
    pub page_size: u64,
    pub max_pfn: u64,
}

/// IOAS mapping (Linux `struct iopt_area`).
#[derive(Debug, Clone)]
pub struct IoasMapping {
    pub iova: u64,
    pub length: u64,
    pub user_addr: u64,
    pub flags: u32,
}

/// HWPT (HW Page Table) (Linux `struct iommufd_hw_pagetable`).
pub struct IommufdHwpt {
    pub id: u32,
    pub ctx_id: u32,
    pub ioas_id: u32,
    pub parent_hwpt_id: Option<u32>,
    pub domain_id: u32,
    pub device_ids: Vec<u32>,
    pub auto_domain: bool,
}

/// IOMMUFD device (Linux `struct iommufd_device`).
pub struct IommufdDevice {
    pub id: u32,
    pub ctx_id: u32,
    pub hwpt_id: u32,
    pub dev_obj_id: u32,
    pub group_id: u32,
    pub attach_cookie: u64,
}

/// IOMMUFD operations.
pub struct IommufdOps {
    pub attach: fn(dev_id: u32, hwpt_id: u32) -> Result<(), &'static str>,
    pub detach: fn(dev_id: u32) -> Result<(), &'static str>,
    pub map_pages: fn(
        ioas_id: u32,
        iova: u64,
        user_addr: u64,
        length: u64,
        flags: u32,
    ) -> Result<(), &'static str>,
    pub unmap_pages: fn(ioas_id: u32, iova: u64, length: u64) -> Result<u64, &'static str>,
    pub copy: fn(
        dst_ioas: u32,
        src_ioas: u32,
        src_iova: u64,
        dst_iova: u64,
        length: u64,
    ) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTX_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static IOAS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static HWPT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IOMMUFD_CTXS: RwLock<BTreeMap<u32, IommufdCtx>> = RwLock::new(BTreeMap::new());
static IOMMUFD_IOAS: RwLock<BTreeMap<u32, IommufdIoas>> = RwLock::new(BTreeMap::new());
static IOMMUFD_HWPTS: RwLock<BTreeMap<u32, IommufdHwpt>> = RwLock::new(BTreeMap::new());
static IOMMUFD_DEVS: RwLock<BTreeMap<u32, IommufdDevice>> = RwLock::new(BTreeMap::new());
static IOMMUFD_OPS: RwLock<Option<IommufdOps>> = RwLock::new(None);

// ── Public API ──────────────────────────────────────────────────────────

/// Register IOMMUFD ops.
pub fn register_ops(ops: IommufdOps) -> Result<(), &'static str> {
    *IOMMUFD_OPS.write() = Some(ops);
    Ok(())
}

/// Create an IOMMUFD context (Linux `iommufd_fops_open`).
pub fn create_ctx() -> Result<u32, &'static str> {
    let id = CTX_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctx = IommufdCtx {
        id,
        ioas_ids: Vec::new(),
        hwpt_ids: Vec::new(),
        device_ids: Vec::new(),
    };
    IOMMUFD_CTXS.write().insert(id, ctx);
    Ok(id)
}

/// Create an IOAS (Linux `IOMMU_IOAS_ALLOC`).
pub fn alloc_ioas(ctx_id: u32) -> Result<u32, &'static str> {
    let id = IOAS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ioas = IommufdIoas {
        id,
        ctx_id,
        name: alloc::format!("ioas-{}", id),
        mappings: BTreeMap::new(),
        page_size: 4096,
        max_pfn: 0xFFFF_FFFF,
    };
    IOMMUFD_IOAS.write().insert(id, ioas);

    let mut ctxs = IOMMUFD_CTXS.write();
    if let Some(ctx) = ctxs.get_mut(&ctx_id) {
        ctx.ioas_ids.push(id);
    }
    Ok(id)
}

/// Map pages into an IOAS (Linux `IOMMU_IOAS_MAP`).
pub fn map_pages(
    ioas_id: u32,
    iova: u64,
    user_addr: u64,
    length: u64,
    flags: u32,
) -> Result<(), &'static str> {
    let map_fn = {
        let ops = IOMMUFD_OPS.read();
        let iommufd_ops = ops.as_ref().ok_or("IOMMUFD ops not registered")?;
        iommufd_ops.map_pages
    };
    (map_fn)(ioas_id, iova, user_addr, length, flags)?;

    let mut ioases = IOMMUFD_IOAS.write();
    let ioas = ioases.get_mut(&ioas_id).ok_or("IOAS not found")?;
    ioas.mappings.insert(
        iova,
        IoasMapping {
            iova,
            length,
            user_addr,
            flags,
        },
    );
    Ok(())
}

/// Unmap pages from an IOAS (Linux `IOMMU_IOAS_UNMAP`).
pub fn unmap_pages(ioas_id: u32, iova: u64, length: u64) -> Result<u64, &'static str> {
    let unmap_fn = {
        let ops = IOMMUFD_OPS.read();
        let iommufd_ops = ops.as_ref().ok_or("IOMMUFD ops not registered")?;
        iommufd_ops.unmap_pages
    };
    let unmapped = (unmap_fn)(ioas_id, iova, length)?;

    let mut ioases = IOMMUFD_IOAS.write();
    if let Some(ioas) = ioases.get_mut(&ioas_id) {
        ioas.mappings.remove(&iova);
    }
    Ok(unmapped)
}

/// Copy mapping between IOAS (Linux `IOMMU_IOAS_MAP` with source IOAS).
pub fn copy_ioas(
    dst_ioas: u32,
    src_ioas: u32,
    src_iova: u64,
    dst_iova: u64,
    length: u64,
) -> Result<(), &'static str> {
    let copy_fn = {
        let ops = IOMMUFD_OPS.read();
        let iommufd_ops = ops.as_ref().ok_or("IOMMUFD ops not registered")?;
        iommufd_ops.copy
    };
    (copy_fn)(dst_ioas, src_ioas, src_iova, dst_iova, length)
}

/// Allocate a HW page table (Linux `IOMMU_HWPT_ALLOC`).
pub fn alloc_hwpt(
    ctx_id: u32,
    ioas_id: u32,
    parent_hwpt_id: Option<u32>,
    auto_domain: bool,
) -> Result<u32, &'static str> {
    let id = HWPT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let hwpt = IommufdHwpt {
        id,
        ctx_id,
        ioas_id,
        parent_hwpt_id,
        domain_id: id,
        device_ids: Vec::new(),
        auto_domain,
    };
    IOMMUFD_HWPTS.write().insert(id, hwpt);

    let mut ctxs = IOMMUFD_CTXS.write();
    if let Some(ctx) = ctxs.get_mut(&ctx_id) {
        ctx.hwpt_ids.push(id);
    }
    Ok(id)
}

/// Attach a device to a HW page table (Linux `IOMMU_HWPT_ATTACH_DEV`).
pub fn attach_device(
    ctx_id: u32,
    hwpt_id: u32,
    dev_obj_id: u32,
    group_id: u32,
) -> Result<u32, &'static str> {
    let attach_fn = {
        let ops = IOMMUFD_OPS.read();
        let iommufd_ops = ops.as_ref().ok_or("IOMMUFD ops not registered")?;
        iommufd_ops.attach
    };

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = IommufdDevice {
        id,
        ctx_id,
        hwpt_id,
        dev_obj_id,
        group_id,
        attach_cookie: id as u64,
    };
    IOMMUFD_DEVS.write().insert(id, dev);

    (attach_fn)(id, hwpt_id)?;

    let mut hwpts = IOMMUFD_HWPTS.write();
    if let Some(hwpt) = hwpts.get_mut(&hwpt_id) {
        hwpt.device_ids.push(id);
    }

    let mut ctxs = IOMMUFD_CTXS.write();
    if let Some(ctx) = ctxs.get_mut(&ctx_id) {
        ctx.device_ids.push(id);
    }
    Ok(id)
}

/// Detach a device from its HW page table (Linux `IOMMU_HWPT_DETACH_DEV`).
pub fn detach_device(dev_id: u32) -> Result<(), &'static str> {
    let detach_fn = {
        let ops = IOMMUFD_OPS.read();
        let iommufd_ops = ops.as_ref().ok_or("IOMMUFD ops not registered")?;
        iommufd_ops.detach
    };
    (detach_fn)(dev_id)?;

    let hwpt_id = {
        let mut devs = IOMMUFD_DEVS.write();
        let dev = devs.remove(&dev_id).ok_or("IOMMUFD device not found")?;
        dev.hwpt_id
    };

    let mut hwpts = IOMMUFD_HWPTS.write();
    if let Some(hwpt) = hwpts.get_mut(&hwpt_id) {
        hwpt.device_ids.retain(|&id| id != dev_id);
    }
    Ok(())
}

/// List all contexts.
pub fn list_ctxs() -> Vec<(u32, usize, usize, usize)> {
    IOMMUFD_CTXS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.ioas_ids.len(), c.hwpt_ids.len(), c.device_ids.len()))
        .collect()
}

/// List IOAS mappings.
pub fn list_mappings(ioas_id: u32) -> Result<Vec<(u64, u64, u64)>, &'static str> {
    let ioases = IOMMUFD_IOAS.read();
    let ioas = ioases.get(&ioas_id).ok_or("IOAS not found")?;
    Ok(ioas
        .mappings
        .iter()
        .map(|(iova, m)| (*iova, m.length, m.user_addr))
        .collect())
}

/// Count registered contexts.
pub fn ctx_count() -> usize {
    IOMMUFD_CTXS.read().len()
}

// ── Software IOMMUFD ────────────────────────────────────────────────────

fn sw_attach(_dev_id: u32, _hwpt_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_detach(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_map_pages(
    _ioas_id: u32,
    _iova: u64,
    _user_addr: u64,
    _length: u64,
    _flags: u32,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_unmap_pages(_ioas_id: u32, _iova: u64, length: u64) -> Result<u64, &'static str> {
    Ok(length)
}
fn sw_copy(
    _dst: u32,
    _src: u32,
    _src_iova: u64,
    _dst_iova: u64,
    _length: u64,
) -> Result<(), &'static str> {
    Ok(())
}

/// Software IOMMUFD ops.
pub fn software_iommufd_ops() -> IommufdOps {
    IommufdOps {
        attach: sw_attach,
        detach: sw_detach,
        map_pages: sw_map_pages,
        unmap_pages: sw_unmap_pages,
        copy: sw_copy,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_iommufd_ops();
    register_ops(ops)?;

    // Create a context
    let ctx_id = create_ctx()?;

    // Allocate an IOAS
    let ioas_id = alloc_ioas(ctx_id)?;

    // Map a page
    map_pages(ioas_id, 0x10000000, 0x20000000, 4096, 0)?;

    // Allocate a HW page table
    let hwpt_id = alloc_hwpt(ctx_id, ioas_id, None, true)?;

    // Attach a device
    let dev_id = attach_device(ctx_id, hwpt_id, 0x100, 1)?;

    // Detach
    detach_device(dev_id)?;

    // Unmap
    unmap_pages(ioas_id, 0x10000000, 4096)?;

    Ok(())
}
