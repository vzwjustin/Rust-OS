//! APM (Advanced Power Management) subsystem
//!
//! Provides APM BIOS interface for power management on legacy systems.
//! Mirrors Linux's `arch/x86/kernel/apm_32.c` and `drivers/apm/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// APM BIOS device (Linux `struct apm_bios_info`).
pub struct ApmBios {
    pub id: u32,
    pub version: u16,
    pub cseg: u16,
    pub dseg: u16,
    pub cseg_len: u16,
    pub dseg_len: u16,
    pub bios_flags: u16,
    pub state: ApmState,
    pub battery: ApmBattery,
    pub ops: ApmOps,
}

/// APM power state (Linux `enum apm_power_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApmState {
    Enabled,
    Disabled,
    Standby,
    Suspend,
    Off,
}

/// APM battery info (Linux `apm_bios_info.battery_flag`).
#[derive(Debug, Clone)]
pub struct ApmBattery {
    pub ac_on_line: bool,
    pub battery_present: bool,
    pub charging: bool,
    pub percentage: u8,
    pub time_left: u32,
}

/// APM operations.
pub struct ApmOps {
    pub set_power_state: fn(dev_id: u32, state: ApmState) -> Result<(), &'static str>,
    pub get_power_status: fn(dev_id: u32) -> Result<ApmBattery, &'static str>,
    pub enable_power_mgmt: fn(dev_id: u32, enable: bool) -> Result<(), &'static str>,
    pub engage_power_mgmt: fn(dev_id: u32, device: u32) -> Result<(), &'static str>,
    pub get_event: fn(dev_id: u32) -> Result<ApmEvent, &'static str>,
    pub standby: fn(dev_id: u32) -> Result<(), &'static str>,
    pub suspend: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// APM event (Linux `enum apm_event_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApmEvent {
    NormalResume,
    CriticalResume,
    Standby,
    Suspend,
    CriticalSuspend,
    UserStandby,
    UserSuspend,
    SystemStandby,
    SystemSuspend,
    PowerStatusChange,
    BatteryLow,
    UpdateTime,
    CriticalSuspendDone,
    NoEvent,
}

/// APM device (Linux `struct apm_user`).
pub struct ApmDevice {
    pub id: u32,
    pub bios_id: u32,
    pub name: String,
    pub state: ApmDevState,
    pub suspends: bool,
    pub standbys: bool,
    pub writer: bool,
    pub reader: bool,
}

/// APM device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApmDevState {
    Normal,
    Pending,
    Busy,
    Suspended,
}

// ── Registry ────────────────────────────────────────────────────────────

static BIOS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static APM_BIOS: RwLock<BTreeMap<u32, ApmBios>> = RwLock::new(BTreeMap::new());
static APM_DEVS: RwLock<BTreeMap<u32, ApmDevice>> = RwLock::new(BTreeMap::new());
static APM_EVENT_QUEUE: RwLock<Vec<ApmEvent>> = RwLock::new(Vec::new());

fn power_mgmt_available(state: ApmState) -> bool {
    !matches!(state, ApmState::Disabled | ApmState::Off)
}

fn find_existing_bios(version: u16, bios_flags: u16) -> Option<u32> {
    APM_BIOS
        .read()
        .iter()
        .find(|(_, bios)| bios.version == version && bios.bios_flags == bios_flags)
        .map(|(id, _)| *id)
}

fn find_existing_device(bios_id: u32, name: &str) -> Option<u32> {
    APM_DEVS
        .read()
        .iter()
        .find(|(_, dev)| dev.bios_id == bios_id && dev.name == name)
        .map(|(id, _)| *id)
}

// ── Public API ──────────────────────────────────────────────────────────

/// Register an APM BIOS (Linux `apm_bios_init`).
pub fn register_bios(version: u16, bios_flags: u16, ops: ApmOps) -> Result<u32, &'static str> {
    if version == 0 {
        return Err("APM BIOS version is invalid");
    }
    if let Some(id) = find_existing_bios(version, bios_flags) {
        return Ok(id);
    }

    let id = BIOS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bios = ApmBios {
        id,
        version,
        cseg: 0,
        dseg: 0,
        cseg_len: 0,
        dseg_len: 0,
        bios_flags,
        state: ApmState::Enabled,
        battery: ApmBattery {
            ac_on_line: true,
            battery_present: false,
            charging: false,
            percentage: 100,
            time_left: 0,
        },
        ops,
    };
    APM_BIOS.write().insert(id, bios);
    Ok(id)
}

/// Set power state (Linux `apm_set_power_state`).
pub fn set_power_state(bios_id: u32, state: ApmState) -> Result<(), &'static str> {
    let set_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        bios.ops.set_power_state
    };
    (set_fn)(bios_id, state)?;

    let mut bioses = APM_BIOS.write();
    if let Some(bios) = bioses.get_mut(&bios_id) {
        bios.state = state;
    }
    Ok(())
}

/// Get power status (Linux `apm_get_power_status`).
pub fn get_power_status(bios_id: u32) -> Result<ApmBattery, &'static str> {
    let get_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        bios.ops.get_power_status
    };
    let battery = (get_fn)(bios_id)?;

    let mut bioses = APM_BIOS.write();
    if let Some(bios) = bioses.get_mut(&bios_id) {
        bios.battery = battery.clone();
    }
    Ok(battery)
}

/// Enable power management (Linux `apm_enable_power_management`).
pub fn enable_power_mgmt(bios_id: u32, enable: bool) -> Result<(), &'static str> {
    let enable_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        bios.ops.enable_power_mgmt
    };
    (enable_fn)(bios_id, enable)?;

    let mut bioses = APM_BIOS.write();
    if let Some(bios) = bioses.get_mut(&bios_id) {
        bios.state = if enable {
            ApmState::Enabled
        } else {
            ApmState::Disabled
        };
    }
    Ok(())
}

/// Standby (Linux `apm_standby`).
pub fn standby(bios_id: u32) -> Result<(), &'static str> {
    let standby_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        if !power_mgmt_available(bios.state) {
            return Err("APM power management is disabled");
        }
        bios.ops.standby
    };
    (standby_fn)(bios_id)?;

    let mut bioses = APM_BIOS.write();
    if let Some(bios) = bioses.get_mut(&bios_id) {
        bios.state = ApmState::Standby;
    }
    Ok(())
}

/// Suspend (Linux `apm_suspend`).
pub fn suspend(bios_id: u32) -> Result<(), &'static str> {
    let suspend_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        if !power_mgmt_available(bios.state) {
            return Err("APM power management is disabled");
        }
        bios.ops.suspend
    };
    (suspend_fn)(bios_id)?;

    let mut bioses = APM_BIOS.write();
    if let Some(bios) = bioses.get_mut(&bios_id) {
        bios.state = ApmState::Suspend;
    }
    Ok(())
}

/// Get pending event (Linux `apm_get_event`).
pub fn get_event(bios_id: u32) -> Result<ApmEvent, &'static str> {
    let get_fn = {
        let bioses = APM_BIOS.read();
        let bios = bioses.get(&bios_id).ok_or("APM BIOS not found")?;
        bios.ops.get_event
    };
    (get_fn)(bios_id)
}

/// Queue an APM event.
pub fn queue_event(event: ApmEvent) {
    APM_EVENT_QUEUE.write().push(event);
}

/// Poll the event queue.
pub fn poll_events() -> Vec<ApmEvent> {
    let mut queue = APM_EVENT_QUEUE.write();
    let events = queue.clone();
    queue.clear();
    events
}

/// Register an APM device (Linux `apm_open`).
pub fn register_device(bios_id: u32, name: &str) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("APM device name is empty");
    }
    if !APM_BIOS.read().contains_key(&bios_id) {
        return Err("APM BIOS not found");
    }
    if let Some(id) = find_existing_device(bios_id, name) {
        return Ok(id);
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = ApmDevice {
        id,
        bios_id,
        name: String::from(name),
        state: ApmDevState::Normal,
        suspends: true,
        standbys: true,
        writer: false,
        reader: true,
    };
    APM_DEVS.write().insert(id, dev);
    Ok(id)
}

/// List all APM BIOS instances.
pub fn list_bios() -> Vec<(u32, u16, ApmState, bool)> {
    APM_BIOS
        .read()
        .iter()
        .map(|(id, b)| (*id, b.version, b.state, b.battery.ac_on_line))
        .collect()
}

/// Count registered BIOS instances.
pub fn bios_count() -> usize {
    APM_BIOS.read().len()
}

// ── Software APM ────────────────────────────────────────────────────────

fn sw_set_power_state(_bios_id: u32, _state: ApmState) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_power_status(_bios_id: u32) -> Result<ApmBattery, &'static str> {
    Ok(ApmBattery {
        ac_on_line: true,
        battery_present: true,
        charging: false,
        percentage: 87,
        time_left: 7200,
    })
}
fn sw_enable_pm(_bios_id: u32, _enable: bool) -> Result<(), &'static str> {
    Ok(())
}
fn sw_engage_pm(_bios_id: u32, _device: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_event(_bios_id: u32) -> Result<ApmEvent, &'static str> {
    Ok(ApmEvent::NoEvent)
}
fn sw_standby(_bios_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_suspend(_bios_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software APM ops.
pub fn software_apm_ops() -> ApmOps {
    ApmOps {
        set_power_state: sw_set_power_state,
        get_power_status: sw_get_power_status,
        enable_power_mgmt: sw_enable_pm,
        engage_power_mgmt: sw_engage_pm,
        get_event: sw_get_event,
        standby: sw_standby,
        suspend: sw_suspend,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !APM_BIOS.read().is_empty() {
        return Ok(());
    }

    let ops = software_apm_ops();
    let bios_id = register_bios(0x0102, 0, ops)?;
    crate::serial_println!("apm: software BIOS registered (id={}, v1.2)", bios_id);
    Ok(())
}
