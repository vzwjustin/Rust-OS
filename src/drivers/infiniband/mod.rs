//! InfiniBand subsystem
//!
//! Provides InfiniBand/RDMA framework for high-performance remote memory access.
//! Mirrors Linux's `drivers/infiniband/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// InfiniBand device (Linux `struct ib_device`).
pub struct IbDevice {
    pub id: u32,
    pub name: String,
    pub node_type: IbNodeType,
    pub transport: IbTransport,
    pub node_guid: u64,
    pub port_count: u8,
    pub ops: IbOps,
    pub state: IbDevState,
    pub port_ids: Vec<u32>,
}

/// IB node type (Linux `enum rdma_node_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbNodeType {
    Ca, // Channel Adapter
    Switch,
    Router,
    Rnic,
}

/// IB transport (Linux `enum rdma_transport`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbTransport {
    Ib,
    Iwarp,
    Usnic,
    UsnicUdp,
    Opa,
}

/// IB device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbDevState {
    Unregistered,
    Registered,
    Active,
    Down,
}

/// IB port (Linux `struct ib_port_attr`).
pub struct IbPort {
    pub id: u32,
    pub dev_id: u32,
    pub port_num: u8,
    pub state: IbPortState,
    pub max_mtu: IbMtu,
    pub active_mtu: IbMtu,
    pub gid_tbl_len: u16,
    pub port_cap_flags: u32,
    pub max_msg_sz: u32,
    pub bad_pkey_cntr: u16,
    pub qkey_viol_cntr: u16,
    pub pkey_tbl_len: u16,
    pub lid: u16,
    pub sm_lid: u16,
    pub lmc: u8,
    pub max_vl_num: u8,
    pub sm_sl: u8,
    pub subnet_timeout: u8,
    pub init_type_reply: u8,
    pub active_width: u8,
    pub active_speed: u8,
    pub phys_state: u8,
}

/// IB port state (Linux `enum ib_port_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbPortState {
    Nop,
    Down,
    Init,
    Armed,
    Active,
    ActiveDefer,
}

/// IB MTU (Linux `enum ib_mtu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbMtu {
    Mtu256,
    Mtu512,
    Mtu1024,
    Mtu2048,
    Mtu4096,
}

/// IB operations (Linux `struct ib_device_ops` subset).
pub struct IbOps {
    pub query_port: fn(dev_id: u32, port_num: u8) -> Result<IbPortAttr, &'static str>,
    pub modify_port: fn(dev_id: u32, port_num: u8, attr: &IbPortAttr) -> Result<(), &'static str>,
    pub get_port_immutable: fn(dev_id: u32, port_num: u8) -> Result<IbPortImmutable, &'static str>,
    pub alloc_pd: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub dealloc_pd: fn(pd_id: u32) -> Result<(), &'static str>,
    pub create_cq: fn(dev_id: u32, cqe: u32) -> Result<u32, &'static str>,
    pub destroy_cq: fn(cq_id: u32) -> Result<(), &'static str>,
    pub create_qp: fn(pd_id: u32, attr: &IbQpInitAttr) -> Result<u32, &'static str>,
    pub destroy_qp: fn(qp_id: u32) -> Result<(), &'static str>,
    pub reg_mr: fn(pd_id: u32, addr: u64, length: u64, access: u32) -> Result<u64, &'static str>,
    pub dereg_mr: fn(mr_handle: u64) -> Result<(), &'static str>,
    pub post_send: fn(qp_id: u32, wr: &IbSendWr) -> Result<(), &'static str>,
    pub post_recv: fn(qp_id: u32, wr: &IbRecvWr) -> Result<(), &'static str>,
    pub poll_cq: fn(cq_id: u32) -> Result<Option<IbWc>, &'static str>,
}

/// IB port attributes (Linux `struct ib_port_attr`).
#[derive(Debug, Clone)]
pub struct IbPortAttr {
    pub state: IbPortState,
    pub max_mtu: IbMtu,
    pub active_mtu: IbMtu,
    pub gid_tbl_len: u16,
    pub port_cap_flags: u32,
    pub max_msg_sz: u32,
    pub pkey_tbl_len: u16,
    pub lid: u16,
    pub sm_lid: u16,
    pub lmc: u8,
    pub active_width: u8,
    pub active_speed: u8,
    pub phys_state: u8,
}

/// IB port immutable (Linux `struct ib_port_immutable`).
#[derive(Debug, Clone)]
pub struct IbPortImmutable {
    pub gid_tbl_len: u16,
    pub pkey_tbl_len: u16,
    pub core_cap_flags: u32,
}

/// IB QP init attributes (Linux `struct ib_qp_init_attr`).
#[derive(Debug, Clone)]
pub struct IbQpInitAttr {
    pub send_cq: u32,
    pub recv_cq: u32,
    pub cap: IbQpCap,
    pub qp_type: IbQpType,
}

/// IB QP capabilities (Linux `struct ib_qp_cap`).
#[derive(Debug, Clone, Copy)]
pub struct IbQpCap {
    pub max_send_wr: u32,
    pub max_recv_wr: u32,
    pub max_send_sge: u32,
    pub max_recv_sge: u32,
    pub max_inline_data: u32,
}

/// IB QP type (Linux `enum ib_qp_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbQpType {
    Rc, // Reliable Connection
    Uc, // Unreliable Connection
    Ud, // Unreliable Datagram
    Raw,
    Xrc,
    QptDriverPriv,
}

/// IB send work request (Linux `struct ib_send_wr`).
#[derive(Debug, Clone)]
pub struct IbSendWr {
    pub wr_id: u64,
    pub opcode: IbWrOp,
    pub send_flags: u32,
    pub length: u32,
    pub remote_addr: u64,
    pub rkey: u32,
}

/// IB recv work request (Linux `struct ib_recv_wr`).
#[derive(Debug, Clone)]
pub struct IbRecvWr {
    pub wr_id: u64,
    pub length: u32,
}

/// IB work completion (Linux `struct ib_wc`).
#[derive(Debug, Clone)]
pub struct IbWc {
    pub wr_id: u64,
    pub status: IbWcStatus,
    pub opcode: IbWrOp,
    pub byte_len: u32,
}

/// IB WC status (Linux `enum ib_wc_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbWcStatus {
    Success,
    LocLenErr,
    LocQpOpErr,
    LocEecOpErr,
    LocProtErr,
    WrFlushErr,
    MwBindErr,
    BadRespErr,
    LocAccessErr,
    RemInvalReqErr,
    RemAccessErr,
    RemOpErr,
    RetryExc,
    RnrRetryExc,
    LocRddViolationErr,
    RemInvRkeyErr,
    RemOpRkeyErr,
    RemInvReadRkeyErr,
    RemAccErr,
    RemAbortErr,
    InvEecnErr,
    InvEecStateErr,
    FatalErr,
    RespTimeoutErr,
    GeneralErr,
}

/// IB WR opcode (Linux `enum ib_wr_opcode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbWrOp {
    Send,
    SendWithImm,
    RdmaWrite,
    RdmaWriteWithImm,
    RdmaRead,
    AtomicCmpAndSwp,
    AtomicFetchAndAdd,
    LocalInv,
    BindMw,
    Receive,
}

/// Protection Domain (Linux `struct ib_pd`).
pub struct IbPd {
    pub id: u32,
    pub dev_id: u32,
}

/// Completion Queue (Linux `struct ib_cq`).
pub struct IbCq {
    pub id: u32,
    pub dev_id: u32,
    pub cqe: u32,
}

/// Queue Pair (Linux `struct ib_qp`).
pub struct IbQp {
    pub id: u32,
    pub pd_id: u32,
    pub qp_num: u32,
    pub state: IbQpState,
}

/// IB QP state (Linux `enum ib_qp_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbQpState {
    Reset,
    Init,
    Rtr,
    Rts,
    Sqd,
    Sqe,
    Err,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CQ_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static QP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IB_DEVS: RwLock<BTreeMap<u32, IbDevice>> = RwLock::new(BTreeMap::new());
static IB_PORTS: RwLock<BTreeMap<u32, IbPort>> = RwLock::new(BTreeMap::new());
static IB_PDS: RwLock<BTreeMap<u32, IbPd>> = RwLock::new(BTreeMap::new());
static IB_CQS: RwLock<BTreeMap<u32, IbCq>> = RwLock::new(BTreeMap::new());
static IB_QPS: RwLock<BTreeMap<u32, IbQp>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an IB device (Linux `ib_register_device`).
pub fn register_device(
    name: &str,
    node_type: IbNodeType,
    transport: IbTransport,
    node_guid: u64,
    port_count: u8,
    ops: IbOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = IbDevice {
        id,
        name: String::from(name),
        node_type,
        transport,
        node_guid,
        port_count,
        ops,
        state: IbDevState::Registered,
        port_ids: Vec::new(),
    };
    IB_DEVS.write().insert(id, dev);

    // Create port entries
    for port_num in 1..=port_count {
        let port_id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let port = IbPort {
            id: port_id,
            dev_id: id,
            port_num,
            state: IbPortState::Down,
            max_mtu: IbMtu::Mtu4096,
            active_mtu: IbMtu::Mtu4096,
            gid_tbl_len: 16,
            port_cap_flags: 0x0260,
            max_msg_sz: 0x40000000,
            bad_pkey_cntr: 0,
            qkey_viol_cntr: 0,
            pkey_tbl_len: 64,
            lid: 0,
            sm_lid: 0,
            lmc: 0,
            max_vl_num: 4,
            sm_sl: 0,
            subnet_timeout: 18,
            init_type_reply: 0,
            active_width: 4,
            active_speed: 5,
            phys_state: 5,
        };
        IB_PORTS.write().insert(port_id, port);

        let mut devs = IB_DEVS.write();
        if let Some(dev) = devs.get_mut(&id) {
            dev.port_ids.push(port_id);
        }
    }

    Ok(id)
}

/// Query port attributes (Linux `ib_query_port`).
pub fn query_port(dev_id: u32, port_num: u8) -> Result<IbPortAttr, &'static str> {
    let query_fn = {
        let devs = IB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("IB device not found")?;
        dev.ops.query_port
    };
    (query_fn)(dev_id, port_num)
}

/// Allocate a Protection Domain (Linux `ib_alloc_pd`).
pub fn alloc_pd(dev_id: u32) -> Result<u32, &'static str> {
    let alloc_fn = {
        let devs = IB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("IB device not found")?;
        dev.ops.alloc_pd
    };
    let pd_id = (alloc_fn)(dev_id)?;

    let pd = IbPd { id: pd_id, dev_id };
    IB_PDS.write().insert(pd_id, pd);
    Ok(pd_id)
}

/// Deallocate a PD (Linux `ib_dealloc_pd`).
pub fn dealloc_pd(pd_id: u32) -> Result<(), &'static str> {
    let dealloc_fn = {
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let pd = pds.get(&pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.dealloc_pd
    };
    (dealloc_fn)(pd_id)?;
    IB_PDS.write().remove(&pd_id);
    Ok(())
}

/// Create a Completion Queue (Linux `ib_create_cq`).
pub fn create_cq(dev_id: u32, cqe: u32) -> Result<u32, &'static str> {
    let create_fn = {
        let devs = IB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("IB device not found")?;
        dev.ops.create_cq
    };
    let cq_id = (create_fn)(dev_id, cqe)?;

    let cq = IbCq {
        id: cq_id,
        dev_id,
        cqe,
    };
    IB_CQS.write().insert(cq_id, cq);
    Ok(cq_id)
}

/// Destroy a CQ (Linux `ib_destroy_cq`).
pub fn destroy_cq(cq_id: u32) -> Result<(), &'static str> {
    let destroy_fn = {
        let cqs = IB_CQS.read();
        let devs = IB_DEVS.read();
        let cq = cqs.get(&cq_id).ok_or("IB CQ not found")?;
        let dev = devs.get(&cq.dev_id).ok_or("IB device not found")?;
        dev.ops.destroy_cq
    };
    (destroy_fn)(cq_id)?;
    IB_CQS.write().remove(&cq_id);
    Ok(())
}

/// Create a Queue Pair (Linux `ib_create_qp`).
pub fn create_qp(pd_id: u32, attr: &IbQpInitAttr) -> Result<u32, &'static str> {
    let create_fn = {
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let pd = pds.get(&pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.create_qp
    };
    let qp_id = (create_fn)(pd_id, attr)?;

    let qp = IbQp {
        id: qp_id,
        pd_id,
        qp_num: qp_id,
        state: IbQpState::Reset,
    };
    IB_QPS.write().insert(qp_id, qp);
    Ok(qp_id)
}

/// Destroy a QP (Linux `ib_destroy_qp`).
pub fn destroy_qp(qp_id: u32) -> Result<(), &'static str> {
    let destroy_fn = {
        let qps = IB_QPS.read();
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let qp = qps.get(&qp_id).ok_or("IB QP not found")?;
        let pd = pds.get(&qp.pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.destroy_qp
    };
    (destroy_fn)(qp_id)?;
    IB_QPS.write().remove(&qp_id);
    Ok(())
}

/// Register a Memory Region (Linux `ib_reg_mr`).
pub fn reg_mr(pd_id: u32, addr: u64, length: u64, access: u32) -> Result<u64, &'static str> {
    let reg_fn = {
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let pd = pds.get(&pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.reg_mr
    };
    (reg_fn)(pd_id, addr, length, access)
}

/// Deregister a Memory Region (Linux `ib_dereg_mr`).
pub fn dereg_mr(mr_handle: u64) -> Result<(), &'static str> {
    // Find the device that owns this MR - in software, we just call dereg
    let dereg_fn = {
        let devs = IB_DEVS.read();
        let dev = devs.iter().next().ok_or("No IB devices")?.1;
        dev.ops.dereg_mr
    };
    (dereg_fn)(mr_handle)
}

/// Post a send work request (Linux `ib_post_send`).
pub fn post_send(qp_id: u32, wr: &IbSendWr) -> Result<(), &'static str> {
    let post_fn = {
        let qps = IB_QPS.read();
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let qp = qps.get(&qp_id).ok_or("IB QP not found")?;
        let pd = pds.get(&qp.pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.post_send
    };
    (post_fn)(qp_id, wr)
}

/// Post a receive work request (Linux `ib_post_recv`).
pub fn post_recv(qp_id: u32, wr: &IbRecvWr) -> Result<(), &'static str> {
    let post_fn = {
        let qps = IB_QPS.read();
        let pds = IB_PDS.read();
        let devs = IB_DEVS.read();
        let qp = qps.get(&qp_id).ok_or("IB QP not found")?;
        let pd = pds.get(&qp.pd_id).ok_or("IB PD not found")?;
        let dev = devs.get(&pd.dev_id).ok_or("IB device not found")?;
        dev.ops.post_recv
    };
    (post_fn)(qp_id, wr)
}

/// Poll a CQ for completions (Linux `ib_poll_cq`).
pub fn poll_cq(cq_id: u32) -> Result<Option<IbWc>, &'static str> {
    let poll_fn = {
        let cqs = IB_CQS.read();
        let devs = IB_DEVS.read();
        let cq = cqs.get(&cq_id).ok_or("IB CQ not found")?;
        let dev = devs.get(&cq.dev_id).ok_or("IB device not found")?;
        dev.ops.poll_cq
    };
    (poll_fn)(cq_id)
}

/// List all IB devices.
pub fn list_devices() -> Vec<(u32, String, IbNodeType, IbDevState, u8)> {
    IB_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.node_type, d.state, d.port_count))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    IB_DEVS.read().len()
}

// ── Software InfiniBand ─────────────────────────────────────────────────

fn sw_query_port(_dev_id: u32, _port_num: u8) -> Result<IbPortAttr, &'static str> {
    Ok(IbPortAttr {
        state: IbPortState::Active,
        max_mtu: IbMtu::Mtu4096,
        active_mtu: IbMtu::Mtu4096,
        gid_tbl_len: 16,
        port_cap_flags: 0x0260,
        max_msg_sz: 0x40000000,
        pkey_tbl_len: 64,
        lid: 1,
        sm_lid: 1,
        lmc: 0,
        active_width: 4,
        active_speed: 5,
        phys_state: 5,
    })
}
fn sw_modify_port(_dev_id: u32, _port_num: u8, _attr: &IbPortAttr) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_port_immutable(_dev_id: u32, _port_num: u8) -> Result<IbPortImmutable, &'static str> {
    Ok(IbPortImmutable {
        gid_tbl_len: 16,
        pkey_tbl_len: 64,
        core_cap_flags: 0x0260,
    })
}
fn sw_alloc_pd(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(PD_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}
fn sw_dealloc_pd(_pd_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_create_cq(_dev_id: u32, _cqe: u32) -> Result<u32, &'static str> {
    Ok(CQ_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}
fn sw_destroy_cq(_cq_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_create_qp(_pd_id: u32, _attr: &IbQpInitAttr) -> Result<u32, &'static str> {
    Ok(QP_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}
fn sw_destroy_qp(_qp_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_reg_mr(_pd_id: u32, _addr: u64, length: u64, _access: u32) -> Result<u64, &'static str> {
    Ok(length)
}
fn sw_dereg_mr(_mr_handle: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_post_send(_qp_id: u32, _wr: &IbSendWr) -> Result<(), &'static str> {
    Ok(())
}
fn sw_post_recv(_qp_id: u32, _wr: &IbRecvWr) -> Result<(), &'static str> {
    Ok(())
}
fn sw_poll_cq(_cq_id: u32) -> Result<Option<IbWc>, &'static str> {
    Ok(Some(IbWc {
        wr_id: 0,
        status: IbWcStatus::Success,
        opcode: IbWrOp::Send,
        byte_len: 0,
    }))
}

/// Software IB ops.
pub fn software_ib_ops() -> IbOps {
    IbOps {
        query_port: sw_query_port,
        modify_port: sw_modify_port,
        get_port_immutable: sw_get_port_immutable,
        alloc_pd: sw_alloc_pd,
        dealloc_pd: sw_dealloc_pd,
        create_cq: sw_create_cq,
        destroy_cq: sw_destroy_cq,
        create_qp: sw_create_qp,
        destroy_qp: sw_destroy_qp,
        reg_mr: sw_reg_mr,
        dereg_mr: sw_dereg_mr,
        post_send: sw_post_send,
        post_recv: sw_post_recv,
        poll_cq: sw_poll_cq,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_ib_ops();
    let dev_id = register_device(
        "sw-ib0",
        IbNodeType::Ca,
        IbTransport::Ib,
        0x0011223344556677,
        1,
        ops,
    )?;

    // Query port
    let port_attr = query_port(dev_id, 1)?;
    let _ = port_attr.state;

    // Allocate PD
    let pd_id = alloc_pd(dev_id)?;

    // Create CQs
    let send_cq = create_cq(dev_id, 64)?;
    let recv_cq = create_cq(dev_id, 64)?;

    // Create QP
    let qp_init = IbQpInitAttr {
        send_cq,
        recv_cq,
        cap: IbQpCap {
            max_send_wr: 16,
            max_recv_wr: 16,
            max_send_sge: 4,
            max_recv_sge: 4,
            max_inline_data: 64,
        },
        qp_type: IbQpType::Rc,
    };
    let qp_id = create_qp(pd_id, &qp_init)?;

    // Register a memory region
    let mr_handle = reg_mr(pd_id, 0x10000000, 4096, 0x0F)?;
    let _ = mr_handle;

    // Post send and recv
    let send_wr = IbSendWr {
        wr_id: 1,
        opcode: IbWrOp::Send,
        send_flags: 0,
        length: 64,
        remote_addr: 0,
        rkey: 0,
    };
    post_send(qp_id, &send_wr)?;

    let recv_wr = IbRecvWr {
        wr_id: 1,
        length: 64,
    };
    post_recv(qp_id, &recv_wr)?;

    // Poll CQ
    let _wc = poll_cq(send_cq)?;

    // Cleanup
    destroy_qp(qp_id)?;
    destroy_cq(send_cq)?;
    destroy_cq(recv_cq)?;
    dealloc_pd(pd_id)?;

    Ok(())
}
