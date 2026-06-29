//! TEE (Trusted Execution Environment) subsystem
//!
//! Provides TEE framework for secure world communication (OP-TEE, AMD-TEE, etc.).
//! Mirrors Linux's `drivers/tee/tee_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// TEE device (Linux `struct tee_device`).
pub struct TeeDevice {
    pub id: u32,
    pub name: String,
    pub desc: TeeDesc,
    pub ops: TeeOps,
    pub state: TeeState,
    pub client_count: u32,
}

/// TEE descriptor (Linux `struct tee_desc`).
#[derive(Debug, Clone)]
pub struct TeeDesc {
    pub name: String,
    pub subsys: String,
    pub dev_type: u32,
    pub flags: u32,
}

/// TEE state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeState {
    Registered,
    Open,
    Closed,
    Error,
}

/// TEE operations (Linux `struct tee_device_ops`).
pub struct TeeOps {
    pub get_version: fn(dev_id: u32) -> Result<TeeVersion, &'static str>,
    pub open: fn(dev_id: u32) -> Result<(), &'static str>,
    pub release: fn(dev_id: u32) -> Result<(), &'static str>,
    pub open_session:
        fn(dev_id: u32, uuid: &[u8; 16], param: &TeeParam) -> Result<u32, &'static str>,
    pub close_session: fn(dev_id: u32, session: u32) -> Result<(), &'static str>,
    pub invoke_func: fn(
        dev_id: u32,
        session: u32,
        func_id: u32,
        param: &TeeParam,
    ) -> Result<TeeParam, &'static str>,
    pub cancel: fn(dev_id: u32, session: u32) -> Result<(), &'static str>,
    pub supp_recv: fn(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub supp_send: fn(dev_id: u32, data: &[u8]) -> Result<usize, &'static str>,
}

/// TEE version info (Linux `struct tee_ioctl_version_data`).
#[derive(Debug, Clone)]
pub struct TeeVersion {
    pub impl_id: u32,
    pub impl_caps: u32,
    pub gen_caps: u32,
}

/// TEE parameter (Linux `struct tee_param`).
#[derive(Debug, Clone)]
pub struct TeeParam {
    pub kind: TeeParamKind,
    pub value: [u64; 4],
    pub buffer: Vec<u8>,
}

/// TEE parameter kind (Linux `enum tee_param_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeParamKind {
    None,
    ValueInput,
    ValueOutput,
    ValueInOut,
    MemrefInput,
    MemrefOutput,
    MemrefInOut,
}

/// TEE context/shm (shared memory) (Linux `struct tee_shm`).
pub struct TeeShm {
    pub id: u32,
    pub dev_id: u32,
    pub size: u64,
    pub flags: u32,
    pub buffer: Vec<u8>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SHM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static TEE_DEVS: RwLock<BTreeMap<u32, TeeDevice>> = RwLock::new(BTreeMap::new());
static TEE_SHMS: RwLock<BTreeMap<u32, TeeShm>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a TEE device.
pub fn register_device(name: &str, desc: TeeDesc, ops: TeeOps) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = TeeDevice {
        id,
        name: String::from(name),
        desc,
        ops,
        state: TeeState::Registered,
        client_count: 0,
    };
    TEE_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Open a TEE device (Linux `tee_open`).
pub fn open_device(dev_id: u32) -> Result<TeeVersion, &'static str> {
    let (open_fn, version_fn) = {
        let devs = TEE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("TEE device not found")?;
        (dev.ops.open, dev.ops.get_version)
    };
    (open_fn)(dev_id)?;

    let mut devs = TEE_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = TeeState::Open;
        dev.client_count += 1;
    }

    (version_fn)(dev_id)
}

/// Release a TEE device (Linux `tee_release`).
pub fn release_device(dev_id: u32) -> Result<(), &'static str> {
    let release_fn = {
        let devs = TEE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("TEE device not found")?;
        dev.ops.release
    };
    (release_fn)(dev_id)?;

    let mut devs = TEE_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        if dev.client_count > 0 {
            dev.client_count -= 1;
        }
        if dev.client_count == 0 {
            dev.state = TeeState::Closed;
        }
    }
    Ok(())
}

/// Open a TEE session (Linux `tee_open_session`).
pub fn open_session(dev_id: u32, uuid: &[u8; 16], param: &TeeParam) -> Result<u32, &'static str> {
    let open_fn = {
        let devs = TEE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("TEE device not found")?;
        if dev.state != TeeState::Open {
            return Err("TEE device not open");
        }
        dev.ops.open_session
    };
    (open_fn)(dev_id, uuid, param)
}

/// Close a TEE session (Linux `tee_close_session`).
pub fn close_session(dev_id: u32, session: u32) -> Result<(), &'static str> {
    let close_fn = {
        let devs = TEE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("TEE device not found")?;
        dev.ops.close_session
    };
    (close_fn)(dev_id, session)
}

/// Invoke a TEE function (Linux `tee_invoke_func`).
pub fn invoke_func(
    dev_id: u32,
    session: u32,
    func_id: u32,
    param: &TeeParam,
) -> Result<TeeParam, &'static str> {
    let invoke_fn = {
        let devs = TEE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("TEE device not found")?;
        if dev.state != TeeState::Open {
            return Err("TEE device not open");
        }
        dev.ops.invoke_func
    };
    (invoke_fn)(dev_id, session, func_id, param)
}

/// Allocate shared memory (Linux `tee_shm_alloc`).
pub fn alloc_shm(dev_id: u32, size: u64, flags: u32) -> Result<u32, &'static str> {
    let shm_id = SHM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let shm = TeeShm {
        id: shm_id,
        dev_id,
        size,
        flags,
        buffer: {
            let mut v = Vec::new();
            v.resize(size as usize, 0);
            v
        },
    };
    TEE_SHMS.write().insert(shm_id, shm);
    Ok(shm_id)
}

/// Free shared memory (Linux `tee_shm_free`).
pub fn free_shm(shm_id: u32) -> Result<(), &'static str> {
    if TEE_SHMS.write().remove(&shm_id).is_none() {
        return Err("TEE shared memory not found");
    }
    Ok(())
}

/// Get shared memory buffer reference.
pub fn get_shm_buffer(shm_id: u32) -> Result<Vec<u8>, &'static str> {
    let shms = TEE_SHMS.read();
    let shm = shms.get(&shm_id).ok_or("TEE shared memory not found")?;
    Ok(shm.buffer.clone())
}

/// List all TEE devices.
pub fn list_devices() -> Vec<(u32, String, TeeState, u32)> {
    TEE_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state, d.client_count))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    TEE_DEVS.read().len()
}

// ── Software TEE ────────────────────────────────────────────────────────

fn sw_get_version(_dev_id: u32) -> Result<TeeVersion, &'static str> {
    Ok(TeeVersion {
        impl_id: 1,
        impl_caps: 0,
        gen_caps: 0x8000_0000,
    })
}
fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_open_session(_dev_id: u32, _uuid: &[u8; 16], _param: &TeeParam) -> Result<u32, &'static str> {
    Ok(1)
}
fn sw_close_session(_dev_id: u32, _session: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_invoke_func(
    _dev_id: u32,
    _session: u32,
    _func_id: u32,
    param: &TeeParam,
) -> Result<TeeParam, &'static str> {
    Ok(param.clone())
}
fn sw_cancel(_dev_id: u32, _session: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_supp_recv(_dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_supp_send(_dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}

/// Software TEE ops.
pub fn software_tee_ops() -> TeeOps {
    TeeOps {
        get_version: sw_get_version,
        open: sw_open,
        release: sw_release,
        open_session: sw_open_session,
        close_session: sw_close_session,
        invoke_func: sw_invoke_func,
        cancel: sw_cancel,
        supp_recv: sw_supp_recv,
        supp_send: sw_supp_send,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("tee: subsystem ready");
    Ok(())
}
