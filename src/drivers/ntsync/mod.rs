//! NTSYNC (NT Synchronization Primitives) subsystem
//!
//! Provides Windows NT-compatible synchronization primitives for
//! Wine/Proton compatibility. Mirrors Linux's `drivers/misc/ntsync.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NT sync primitive type (Linux `enum ntsync_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtSyncType {
    Semaphore,
    Mutex,
    Event,
    Timer,
}

/// NT sync object (Linux `struct ntsync_obj`).
pub struct NtSyncObj {
    pub id: u32,
    pub name: String,
    pub type_: NtSyncType,
    pub manual_reset: bool,
    pub signaled: bool,
    pub count: u32,           // For semaphores
    pub max_count: u32,       // For semaphores
    pub owner_tid: u32,       // For mutexes
    pub recursion_count: u32, // For mutexes
    pub abandoned: bool,      // For mutexes
    pub due_time: u64,        // For timers (ns)
    pub period: u64,          // For timers (ns)
    pub timer_state: bool,    // For timers
}

/// NT sync device (Linux `struct ntsync_device`).
pub struct NtSyncDevice {
    pub id: u32,
    pub obj_ids: Vec<u32>,
}

/// NT sync queue (Linux `struct ntsync_queue`).
pub struct NtSyncQueue {
    pub id: u32,
    pub obj_ids: Vec<u32>,
    pub wake_count: u32,
    pub woken: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static OBJ_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static QUEUE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static NT_OBJS: RwLock<BTreeMap<u32, NtSyncObj>> = RwLock::new(BTreeMap::new());
static NT_DEVS: RwLock<BTreeMap<u32, NtSyncDevice>> = RwLock::new(BTreeMap::new());
static NT_QUEUES: RwLock<BTreeMap<u32, NtSyncQueue>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Create a semaphore (Linux `NTSYNC_CREATE_SEM`).
pub fn create_semaphore(name: &str, initial: u32, max: u32) -> Result<u32, &'static str> {
    if max == 0 {
        return Err("Semaphore max must be non-zero");
    }
    if initial > max {
        return Err("Semaphore initial count exceeds max");
    }
    let id = OBJ_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = NtSyncObj {
        id,
        name: String::from(name),
        type_: NtSyncType::Semaphore,
        manual_reset: false,
        signaled: initial > 0,
        count: initial,
        max_count: max,
        owner_tid: 0,
        recursion_count: 0,
        abandoned: false,
        due_time: 0,
        period: 0,
        timer_state: false,
    };
    NT_OBJS.write().insert(id, obj);
    Ok(id)
}

/// Create a mutex (Linux `NTSYNC_CREATE_MUTEX`).
pub fn create_mutex(name: &str, owner_tid: u32) -> Result<u32, &'static str> {
    let id = OBJ_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = NtSyncObj {
        id,
        name: String::from(name),
        type_: NtSyncType::Mutex,
        manual_reset: false,
        signaled: owner_tid == 0,
        count: if owner_tid != 0 { 1 } else { 0 },
        max_count: 1,
        owner_tid,
        recursion_count: if owner_tid != 0 { 1 } else { 0 },
        abandoned: false,
        due_time: 0,
        period: 0,
        timer_state: false,
    };
    NT_OBJS.write().insert(id, obj);
    Ok(id)
}

/// Create an event (Linux `NTSYNC_CREATE_EVENT`).
pub fn create_event(name: &str, manual_reset: bool, signaled: bool) -> Result<u32, &'static str> {
    let id = OBJ_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = NtSyncObj {
        id,
        name: String::from(name),
        type_: NtSyncType::Event,
        manual_reset,
        signaled,
        count: 0,
        max_count: 0,
        owner_tid: 0,
        recursion_count: 0,
        abandoned: false,
        due_time: 0,
        period: 0,
        timer_state: false,
    };
    NT_OBJS.write().insert(id, obj);
    Ok(id)
}

/// Create a timer (Linux `NTSYNC_CREATE_TIMER`).
pub fn create_timer(name: &str, manual_reset: bool) -> Result<u32, &'static str> {
    let id = OBJ_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = NtSyncObj {
        id,
        name: String::from(name),
        type_: NtSyncType::Timer,
        manual_reset,
        signaled: false,
        count: 0,
        max_count: 0,
        owner_tid: 0,
        recursion_count: 0,
        abandoned: false,
        due_time: 0,
        period: 0,
        timer_state: false,
    };
    NT_OBJS.write().insert(id, obj);
    Ok(id)
}

/// Signal a semaphore (Linux `NTSYNC_SEM_POST`).
pub fn semaphore_post(obj_id: u32, count: u32) -> Result<u32, &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Semaphore {
        return Err("Object is not a semaphore");
    }
    let new_count = obj
        .count
        .checked_add(count)
        .ok_or("Semaphore count overflow")?;
    if new_count > obj.max_count {
        return Err("Semaphore count exceeds max");
    }
    obj.count = new_count;
    obj.signaled = new_count > 0;
    Ok(new_count)
}

/// Wait on a semaphore (decrement count).
pub fn semaphore_wait(obj_id: u32) -> Result<(), &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Semaphore {
        return Err("Object is not a semaphore");
    }
    if obj.count == 0 {
        return Err("Semaphore not signaled");
    }
    obj.count -= 1;
    obj.signaled = obj.count > 0;
    Ok(())
}

/// Release a mutex (Linux `NTSYNC_MUTEX_RELEASE`).
pub fn mutex_release(obj_id: u32, tid: u32) -> Result<u32, &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Mutex {
        return Err("Object is not a mutex");
    }
    if obj.owner_tid != tid {
        return Err("Mutex not owned by this thread");
    }
    if obj.recursion_count > 1 {
        obj.recursion_count -= 1;
        return Ok(obj.recursion_count);
    }
    obj.owner_tid = 0;
    obj.recursion_count = 0;
    obj.signaled = true;
    Ok(0)
}

/// Acquire a mutex.
pub fn mutex_acquire(obj_id: u32, tid: u32) -> Result<(), &'static str> {
    if tid == 0 {
        return Err("Mutex owner thread id must be non-zero");
    }
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Mutex {
        return Err("Object is not a mutex");
    }
    if obj.owner_tid == tid {
        obj.recursion_count = obj
            .recursion_count
            .checked_add(1)
            .ok_or("Mutex recursion count overflow")?;
        return Ok(());
    }
    if obj.owner_tid != 0 {
        return Err("Mutex owned by another thread");
    }
    obj.owner_tid = tid;
    obj.recursion_count = 1;
    obj.signaled = false;
    Ok(())
}

/// Set event (Linux `NTSYNC_EVENT_SET`).
pub fn event_set(obj_id: u32) -> Result<(), &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Event {
        return Err("Object is not an event");
    }
    obj.signaled = true;
    Ok(())
}

/// Reset event (Linux `NTSYNC_EVENT_RESET`).
pub fn event_reset(obj_id: u32) -> Result<bool, &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Event {
        return Err("Object is not an event");
    }
    let was_signaled = obj.signaled;
    obj.signaled = false;
    Ok(was_signaled)
}

/// Pulse event (Linux `NTSYNC_EVENT_PULSE`).
pub fn event_pulse(obj_id: u32) -> Result<(), &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Event {
        return Err("Object is not an event");
    }
    obj.signaled = false;
    Ok(())
}

/// Set timer (Linux `NTSYNC_TIMER_SET`).
pub fn timer_set(obj_id: u32, due_time_ns: u64, period_ns: u64) -> Result<(), &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Timer {
        return Err("Object is not a timer");
    }
    obj.due_time = due_time_ns;
    obj.period = period_ns;
    obj.timer_state = true;
    Ok(())
}

/// Cancel timer (Linux `NTSYNC_TIMER_CANCEL`).
pub fn timer_cancel(obj_id: u32) -> Result<bool, &'static str> {
    let mut objs = NT_OBJS.write();
    let obj = objs.get_mut(&obj_id).ok_or("NT sync object not found")?;
    if obj.type_ != NtSyncType::Timer {
        return Err("Object is not a timer");
    }
    let was_signaled = obj.signaled;
    obj.timer_state = false;
    obj.signaled = false;
    Ok(was_signaled)
}

/// Create a sync device (Linux `NTSYNC_CREATE_DEV`).
pub fn create_device() -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = NtSyncDevice {
        id,
        obj_ids: Vec::new(),
    };
    NT_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Create a wait queue (Linux `NTSYNC_WAIT_ALL` / `NTSYNC_WAIT_ANY`).
pub fn create_queue(obj_ids: Vec<u32>) -> Result<u32, &'static str> {
    if obj_ids.is_empty() {
        return Err("NT sync queue must contain at least one object");
    }
    {
        let objs = NT_OBJS.read();
        for (idx, obj_id) in obj_ids.iter().enumerate() {
            if !objs.contains_key(obj_id) {
                return Err("NT sync queue references missing object");
            }
            if obj_ids.iter().skip(idx + 1).any(|other| other == obj_id) {
                return Err("NT sync queue contains duplicate object");
            }
        }
    }

    let id = QUEUE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let queue = NtSyncQueue {
        id,
        obj_ids,
        wake_count: 0,
        woken: false,
    };
    NT_QUEUES.write().insert(id, queue);
    Ok(id)
}

/// Check if any object in a queue is signaled.
pub fn queue_check_signaled(queue_id: u32) -> Result<bool, &'static str> {
    let queues = NT_QUEUES.read();
    let queue = queues.get(&queue_id).ok_or("NT sync queue not found")?;
    let objs = NT_OBJS.read();
    for &oid in &queue.obj_ids {
        let obj = objs
            .get(&oid)
            .ok_or("NT sync queue references missing object")?;
        if obj.signaled {
            return Ok(true);
        }
    }
    Ok(false)
}

/// List all sync objects.
pub fn list_objects() -> Vec<(u32, String, NtSyncType, bool)> {
    NT_OBJS
        .read()
        .iter()
        .map(|(id, o)| (*id, o.name.clone(), o.type_, o.signaled))
        .collect()
}

/// Count registered objects.
pub fn object_count() -> usize {
    NT_OBJS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ntsync: framework ready");
    Ok(())
}
