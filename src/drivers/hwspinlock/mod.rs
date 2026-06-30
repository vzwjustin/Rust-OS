//! Hwspinlock subsystem
//!
//! Provides hardware spinlock framework for mutual exclusion across processors
//! or between a CPU and a coprocessor. Mirrors Linux's `drivers/hwspinlock/hwspinlock_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Hwspinlock operations (Linux `struct hwspinlock_ops`).
pub struct HwspinlockOps {
    pub trylock: fn(lock_id: u32) -> Result<bool, &'static str>,
    pub unlock: fn(lock_id: u32) -> Result<(), &'static str>,
    pub test_set: fn(lock_id: u32) -> Result<bool, &'static str>,
    pub relax: fn(lock_id: u32) -> Result<(), &'static str>,
}

/// Hwspinlock instance (Linux `struct hwspinlock`).
pub struct Hwspinlock {
    pub id: u32,
    pub ops: HwspinlockOps,
    pub locked: bool,
    pub owner: Option<u32>,
}

/// Hwspinlock controller/bank (Linux `struct hwspinlock_device`).
pub struct HwspinlockDevice {
    pub name: String,
    pub base_id: u32,
    pub num_locks: u32,
    pub lock_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static LOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HWSPINLOCKS: RwLock<BTreeMap<u32, Hwspinlock>> = RwLock::new(BTreeMap::new());
static HWSPINLOCK_DEVICES: RwLock<BTreeMap<u32, HwspinlockDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a hwspinlock device (bank of locks).
pub fn register_device(
    name: &str,
    ops: HwspinlockOps,
    num_locks: u32,
) -> Result<u32, &'static str> {
    let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let base_id = LOCK_ID_COUNTER.fetch_add(0, Ordering::SeqCst);
    let mut lock_ids = Vec::new();

    for _i in 0..num_locks {
        let lock_id = LOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let lock = Hwspinlock {
            id: lock_id,
            ops: HwspinlockOps {
                trylock: ops.trylock,
                unlock: ops.unlock,
                test_set: ops.test_set,
                relax: ops.relax,
            },
            locked: false,
            owner: None,
        };
        HWSPINLOCKS.write().insert(lock_id, lock);
        lock_ids.push(lock_id);
    }

    let device = HwspinlockDevice {
        name: String::from(name),
        base_id,
        num_locks,
        lock_ids,
    };
    HWSPINLOCK_DEVICES.write().insert(dev_id, device);
    Ok(dev_id)
}

/// Try to acquire a hwspinlock (non-blocking) (Linux `hwspin_trylock`).
pub fn trylock(lock_id: u32, owner: u32) -> Result<bool, &'static str> {
    let trylock_fn = {
        let locks = HWSPINLOCKS.read();
        let lock = locks.get(&lock_id).ok_or("Hwspinlock not found")?;
        lock.ops.trylock
    };

    let acquired = (trylock_fn)(lock_id)?;
    if acquired {
        let mut locks = HWSPINLOCKS.write();
        if let Some(lock) = locks.get_mut(&lock_id) {
            lock.locked = true;
            lock.owner = Some(owner);
        }
    }
    Ok(acquired)
}

/// Acquire a hwspinlock (blocking with timeout) (Linux `hwspin_lock_timeout`).
pub fn lock_timeout(lock_id: u32, owner: u32, timeout_us: u64) -> Result<(), &'static str> {
    let (trylock_fn, relax_fn) = {
        let locks = HWSPINLOCKS.read();
        let lock = locks.get(&lock_id).ok_or("Hwspinlock not found")?;
        (lock.ops.trylock, lock.ops.relax)
    };

    let mut elapsed: u64 = 0;
    const RELAX_DELAY_US: u64 = 10;

    loop {
        let acquired = (trylock_fn)(lock_id)?;
        if acquired {
            let mut locks = HWSPINLOCKS.write();
            if let Some(lock) = locks.get_mut(&lock_id) {
                lock.locked = true;
                lock.owner = Some(owner);
            }
            return Ok(());
        }

        if elapsed >= timeout_us {
            return Err("Hwspinlock lock timeout");
        }

        (relax_fn)(lock_id)?;
        elapsed += RELAX_DELAY_US;
    }
}

/// Release a hwspinlock (Linux `hwspin_unlock`).
pub fn unlock(lock_id: u32) -> Result<(), &'static str> {
    let unlock_fn = {
        let locks = HWSPINLOCKS.read();
        let lock = locks.get(&lock_id).ok_or("Hwspinlock not found")?;
        if !lock.locked {
            return Err("Hwspinlock not locked");
        }
        lock.ops.unlock
    };

    (unlock_fn)(lock_id)?;

    let mut locks = HWSPINLOCKS.write();
    if let Some(lock) = locks.get_mut(&lock_id) {
        lock.locked = false;
        lock.owner = None;
    }
    Ok(())
}

/// Unlock a hwspinlock and verify owner (Linux `hwspin_unlock_raw`).
pub fn unlock_raw(lock_id: u32, owner: u32) -> Result<(), &'static str> {
    {
        let locks = HWSPINLOCKS.read();
        let lock = locks.get(&lock_id).ok_or("Hwspinlock not found")?;
        if !lock.locked {
            return Err("Hwspinlock not locked");
        }
        if lock.owner != Some(owner) {
            return Err("Hwspinlock owner mismatch");
        }
    }
    unlock(lock_id)
}

/// Test-and-set a hwspinlock atomically.
pub fn test_set(lock_id: u32, owner: u32) -> Result<bool, &'static str> {
    let test_set_fn = {
        let locks = HWSPINLOCKS.read();
        let lock = locks.get(&lock_id).ok_or("Hwspinlock not found")?;
        lock.ops.test_set
    };

    let was_locked = (test_set_fn)(lock_id)?;
    if !was_locked {
        let mut locks = HWSPINLOCKS.write();
        if let Some(lock) = locks.get_mut(&lock_id) {
            lock.locked = true;
            lock.owner = Some(owner);
        }
    }
    Ok(was_locked)
}

/// Get the base ID of a hwspinlock device.
pub fn get_base_id(device_id: u32) -> Result<u32, &'static str> {
    let devices = HWSPINLOCK_DEVICES.read();
    let dev = devices
        .get(&device_id)
        .ok_or("Hwspinlock device not found")?;
    Ok(dev.base_id)
}

/// List all registered hwspinlock devices.
pub fn list_devices() -> Vec<(u32, String, u32)> {
    HWSPINLOCK_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.num_locks))
        .collect()
}

/// Count total registered locks.
pub fn lock_count() -> usize {
    HWSPINLOCKS.read().len()
}

/// Count locked locks.
pub fn locked_count() -> usize {
    HWSPINLOCKS.read().values().filter(|l| l.locked).count()
}

// ── Software hwspinlock ────────────────────────────────────────────────

fn sw_trylock(_lock_id: u32) -> Result<bool, &'static str> {
    let mut locks = HWSPINLOCKS.write();
    if let Some(lock) = locks.values_mut().next() {
        if lock.locked {
            return Ok(false);
        }
        return Ok(true);
    }
    Ok(false)
}

fn sw_unlock(_lock_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_test_set(_lock_id: u32) -> Result<bool, &'static str> {
    Ok(false)
}
fn sw_relax(_lock_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software hwspinlock ops (in-memory spinlock).
pub fn software_hwspinlock_ops() -> HwspinlockOps {
    HwspinlockOps {
        trylock: sw_trylock,
        unlock: sw_unlock,
        test_set: sw_test_set,
        relax: sw_relax,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

/// Number of locks exposed by the built-in software hwspinlock bank.
const SW_HWSPINLOCK_NUM_LOCKS: u32 = 32;

pub fn init() -> Result<(), &'static str> {
    if !HWSPINLOCK_DEVICES.read().is_empty() {
        return Ok(());
    }

    let ops = software_hwspinlock_ops();
    register_device("software-hwspinlock", ops, SW_HWSPINLOCK_NUM_LOCKS)?;
    crate::serial_println!("hwspinlock: subsystem ready");
    Ok(())
}
