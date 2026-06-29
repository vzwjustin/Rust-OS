//! Thunderbolt subsystem
//!
//! Provides Thunderbolt/USB4 domain and device management.
//! Mirrors Linux's `drivers/thunderbolt/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Thunderbolt domain (Linux `struct tb_domain`).
pub struct TbDomain {
    pub id: u32,
    pub name: String,
    pub tbm: u32,
    pub nboot_acl: u32,
    pub cm_mode: bool,
    pub security_level: TbSecurityLevel,
    pub router_ids: Vec<u32>,
    pub tunnel_ids: Vec<u32>,
    pub ops: TbDomainOps,
    pub state: TbDomainState,
}

/// TB security level (Linux `enum tb_security_level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TbSecurityLevel {
    None,
    User,
    Secure,
    DpOnly,
    UsbOnly,
    NoPci,
}

/// TB domain state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TbDomainState {
    Uninitialized,
    Initialized,
    Active,
    Suspended,
}

/// TB domain operations (Linux `struct tb_domain_ops`).
pub struct TbDomainOps {
    pub init: fn(dom_id: u32) -> Result<(), &'static str>,
    pub stop: fn(dom_id: u32) -> Result<(), &'static str>,
    pub suspend: fn(dom_id: u32) -> Result<(), &'static str>,
    pub resume: fn(dom_id: u32) -> Result<(), &'static str>,
    pub approve_switch: fn(dom_id: u32, router_id: u32) -> Result<(), &'static str>,
    pub disconnect_path: fn(dom_id: u32, path_id: u32) -> Result<(), &'static str>,
}

/// Thunderbolt router/switch (Linux `struct tb_switch`).
pub struct TbSwitch {
    pub id: u32,
    pub dom_id: u32,
    pub name: String,
    pub vendor: u16,
    pub device: u16,
    pub revision: u8,
    pub depth: u8,
    pub route: u64,
    pub port_count: u8,
    pub port_ids: Vec<u32>,
    pub parent_id: Option<u32>,
    pub authorized: bool,
    pub state: TbSwitchState,
    pub config: TbSwitchConfig,
}

/// TB switch state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TbSwitchState {
    Unplugged,
    Plugged,
    Authorized,
    Configured,
    Active,
    Removed,
}

/// TB switch config (Linux `struct tb_switch_config`).
#[derive(Debug, Clone)]
pub struct TbSwitchConfig {
    pub vendor_name: String,
    pub device_name: String,
    pub generation: u8,
    pub max_usb4: bool,
    pub nvm_version: u32,
    pub nvm_size: u64,
}

/// TB port (Linux `struct tb_port`).
pub struct TbPort {
    pub id: u32,
    pub switch_id: u32,
    pub port_number: u8,
    pub port_type: TbPortType,
    pub link: bool,
    pub enabled: bool,
    pub dual_link_port: Option<u32>,
    pub cap_usb4: bool,
    pub lane: u8,
    pub width: u8,
    pub speed: u8,
}

/// TB port type (Linux `enum tb_port_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TbPortType {
    In,
    Out,
    Nhi,
    Dp,
    Pcie,
    Usb3,
    TypeC,
}

/// TB tunnel (Linux `struct tb_tunnel`).
pub struct TbTunnel {
    pub id: u32,
    pub dom_id: u32,
    pub src_port: u32,
    pub dst_port: u32,
    pub tunnel_type: TbTunnelType,
    pub bandwidth: u32,
    pub active: bool,
}

/// TB tunnel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TbTunnelType {
    Pcie,
    Dp,
    Usb3,
    Dma,
}

// ── Registry ────────────────────────────────────────────────────────────

static DOM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SW_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static TUNNEL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static TB_DOMAINS: RwLock<BTreeMap<u32, TbDomain>> = RwLock::new(BTreeMap::new());
static TB_SWITCHES: RwLock<BTreeMap<u32, TbSwitch>> = RwLock::new(BTreeMap::new());
static TB_PORTS: RwLock<BTreeMap<u32, TbPort>> = RwLock::new(BTreeMap::new());
static TB_TUNNELS: RwLock<BTreeMap<u32, TbTunnel>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a Thunderbolt domain (Linux `tb_domain_alloc` + `tb_domain_add`).
pub fn register_domain(
    name: &str,
    security: TbSecurityLevel,
    ops: TbDomainOps,
) -> Result<u32, &'static str> {
    let id = DOM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dom = TbDomain {
        id,
        name: String::from(name),
        tbm: 0,
        nboot_acl: 16,
        cm_mode: true,
        security_level: security,
        router_ids: Vec::new(),
        tunnel_ids: Vec::new(),
        ops,
        state: TbDomainState::Uninitialized,
    };
    TB_DOMAINS.write().insert(id, dom);
    Ok(id)
}

/// Initialize a domain (Linux `tb_domain_init`).
pub fn init_domain(dom_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let doms = TB_DOMAINS.read();
        let dom = doms.get(&dom_id).ok_or("TB domain not found")?;
        dom.ops.init
    };
    (init_fn)(dom_id)?;

    let mut doms = TB_DOMAINS.write();
    if let Some(dom) = doms.get_mut(&dom_id) {
        dom.state = TbDomainState::Active;
    }
    Ok(())
}

/// Register a switch (Linux `tb_switch_alloc` + `tb_switch_add`).
pub fn register_switch(
    dom_id: u32,
    vendor: u16,
    device: u16,
    revision: u8,
    depth: u8,
    route: u64,
    port_count: u8,
    parent_id: Option<u32>,
) -> Result<u32, &'static str> {
    let id = SW_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut port_ids = Vec::new();
    for port_num in 1..=port_count {
        let port_id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let port = TbPort {
            id: port_id,
            switch_id: id,
            port_number: port_num,
            port_type: if port_num == 1 {
                TbPortType::In
            } else {
                TbPortType::Out
            },
            link: false,
            enabled: false,
            dual_link_port: None,
            cap_usb4: true,
            lane: 0,
            width: 0,
            speed: 0,
        };
        TB_PORTS.write().insert(port_id, port);
        port_ids.push(port_id);
    }

    let sw = TbSwitch {
        id,
        dom_id,
        name: alloc::format!("sw-{}", id),
        vendor,
        device,
        revision,
        depth,
        route,
        port_count,
        port_ids,
        parent_id,
        authorized: false,
        state: TbSwitchState::Plugged,
        config: TbSwitchConfig {
            vendor_name: String::from("Unknown"),
            device_name: String::from("Unknown"),
            generation: 4,
            max_usb4: true,
            nvm_version: 0,
            nvm_size: 0,
        },
    };
    TB_SWITCHES.write().insert(id, sw);

    let mut doms = TB_DOMAINS.write();
    if let Some(dom) = doms.get_mut(&dom_id) {
        dom.router_ids.push(id);
    }
    Ok(id)
}

/// Authorize a switch (Linux `tb_switch_authorize`).
pub fn authorize_switch(dom_id: u32, sw_id: u32) -> Result<(), &'static str> {
    let approve_fn = {
        let doms = TB_DOMAINS.read();
        let dom = doms.get(&dom_id).ok_or("TB domain not found")?;
        dom.ops.approve_switch
    };
    (approve_fn)(dom_id, sw_id)?;

    let mut switches = TB_SWITCHES.write();
    if let Some(sw) = switches.get_mut(&sw_id) {
        sw.authorized = true;
        sw.state = TbSwitchState::Authorized;
    }
    Ok(())
}

/// Configure a switch (Linux `tb_switch_configure`).
pub fn configure_switch(sw_id: u32) -> Result<(), &'static str> {
    let mut switches = TB_SWITCHES.write();
    let sw = switches.get_mut(&sw_id).ok_or("TB switch not found")?;
    if !sw.authorized {
        return Err("Switch not authorized");
    }
    sw.state = TbSwitchState::Configured;
    Ok(())
}

/// Activate a switch.
pub fn activate_switch(sw_id: u32) -> Result<(), &'static str> {
    let mut switches = TB_SWITCHES.write();
    let sw = switches.get_mut(&sw_id).ok_or("TB switch not found")?;
    if sw.state != TbSwitchState::Configured {
        return Err("Switch not configured");
    }
    sw.state = TbSwitchState::Active;

    // Enable all ports
    let port_ids = sw.port_ids.clone();
    drop(switches);

    let mut ports = TB_PORTS.write();
    for &pid in &port_ids {
        if let Some(port) = ports.get_mut(&pid) {
            port.enabled = true;
            port.link = true;
            port.width = 2;
            port.speed = 20; // 20 Gbps
        }
    }
    Ok(())
}

/// Create a tunnel (Linux `tb_tunnel_alloc_*`).
pub fn create_tunnel(
    dom_id: u32,
    src_port: u32,
    dst_port: u32,
    tunnel_type: TbTunnelType,
    bandwidth: u32,
) -> Result<u32, &'static str> {
    let id = TUNNEL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tunnel = TbTunnel {
        id,
        dom_id,
        src_port,
        dst_port,
        tunnel_type,
        bandwidth,
        active: true,
    };
    TB_TUNNELS.write().insert(id, tunnel);

    let mut doms = TB_DOMAINS.write();
    if let Some(dom) = doms.get_mut(&dom_id) {
        dom.tunnel_ids.push(id);
    }
    Ok(id)
}

/// Destroy a tunnel (Linux `tb_tunnel_deactivate` + `tb_tunnel_free`).
pub fn destroy_tunnel(dom_id: u32, tunnel_id: u32) -> Result<(), &'static str> {
    let disconnect_fn = {
        let doms = TB_DOMAINS.read();
        let dom = doms.get(&dom_id).ok_or("TB domain not found")?;
        dom.ops.disconnect_path
    };
    (disconnect_fn)(dom_id, tunnel_id)?;

    TB_TUNNELS.write().remove(&tunnel_id);

    let mut doms = TB_DOMAINS.write();
    if let Some(dom) = doms.get_mut(&dom_id) {
        dom.tunnel_ids.retain(|&id| id != tunnel_id);
    }
    Ok(())
}

/// List all domains.
pub fn list_domains() -> Vec<(u32, String, TbSecurityLevel, TbDomainState, usize)> {
    TB_DOMAINS
        .read()
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.name.clone(),
                d.security_level,
                d.state,
                d.router_ids.len(),
            )
        })
        .collect()
}

/// List switches in a domain.
pub fn list_switches(
    dom_id: u32,
) -> Result<Vec<(u32, String, u8, u64, TbSwitchState, bool)>, &'static str> {
    let doms = TB_DOMAINS.read();
    let dom = doms.get(&dom_id).ok_or("TB domain not found")?;
    let switches = TB_SWITCHES.read();
    let mut result = Vec::new();
    for &sw_id in &dom.router_ids {
        if let Some(sw) = switches.get(&sw_id) {
            result.push((
                sw.id,
                sw.name.clone(),
                sw.depth,
                sw.route,
                sw.state,
                sw.authorized,
            ));
        }
    }
    Ok(result)
}

/// Count registered domains.
pub fn domain_count() -> usize {
    TB_DOMAINS.read().len()
}

// ── Software Thunderbolt ────────────────────────────────────────────────

fn sw_dom_init(_dom_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dom_stop(_dom_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dom_suspend(_dom_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dom_resume(_dom_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dom_approve(_dom_id: u32, _router_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dom_disconnect(_dom_id: u32, _path_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software TB domain ops.
pub fn software_tb_domain_ops() -> TbDomainOps {
    TbDomainOps {
        init: sw_dom_init,
        stop: sw_dom_stop,
        suspend: sw_dom_suspend,
        resume: sw_dom_resume,
        approve_switch: sw_dom_approve,
        disconnect_path: sw_dom_disconnect,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_tb_domain_ops();
    let dom_id = register_domain("sw-tb-dom0", TbSecurityLevel::User, ops)?;

    // Initialize domain
    init_domain(dom_id)?;

    // Register host router (depth 0)
    let host_sw = register_switch(dom_id, 0x8086, 0x9A1B, 0x01, 0, 0, 4, None)?;

    // Authorize and configure host router
    authorize_switch(dom_id, host_sw)?;
    configure_switch(host_sw)?;
    activate_switch(host_sw)?;

    // Register a device router (depth 1, route 1)
    let dev_sw = register_switch(dom_id, 0x8086, 0x9A1C, 0x01, 1, 1, 4, Some(host_sw))?;

    // Authorize device router
    authorize_switch(dom_id, dev_sw)?;
    configure_switch(dev_sw)?;
    activate_switch(dev_sw)?;

    // Create a PCIe tunnel from host port 3 to device port 1
    let host_port_3 = {
        let switches = TB_SWITCHES.read();
        let sw = switches.get(&host_sw).ok_or("Host switch not found")?;
        sw.port_ids.get(2).copied().ok_or("Port 3 not found")?
    };
    let dev_port_1 = {
        let switches = TB_SWITCHES.read();
        let sw = switches.get(&dev_sw).ok_or("Device switch not found")?;
        sw.port_ids.first().copied().ok_or("Port 1 not found")?
    };
    let tunnel_id = create_tunnel(dom_id, host_port_3, dev_port_1, TbTunnelType::Pcie, 40000)?;

    // Destroy tunnel
    destroy_tunnel(dom_id, tunnel_id)?;

    Ok(())
}
