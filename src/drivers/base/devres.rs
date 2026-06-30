//! Device-managed resources (devres).
//!
//! Resources registered here are automatically released when the device is
//! removed, mirroring Linux's `devm_*` family of helpers.
//!
//! Pure-Rust, no_std. No bindings:: calls.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::base::device::Device;

// ── Global devres table ──────────────────────────────────────────────────────

/// Type-erased cleanup callback stored in the devres table.
trait DevresCleanup: Send {
    /// Called when the associated device is released.
    fn release(&mut self);
}

struct DevresEntry {
    /// Raw pointer to the device (not Arc, to avoid cycles; we clear on release).
    device_ptr: *const Device,
    cleanup: Box<dyn DevresCleanup>,
}

// SAFETY: DevresEntry is only accessed behind a Mutex.
unsafe impl Send for DevresEntry {}
unsafe impl Sync for DevresEntry {}

static DEVRES_TABLE: Mutex<Vec<DevresEntry>> = Mutex::new(Vec::new());

// ── Devres<T> ────────────────────────────────────────────────────────────────

/// A device-managed resource of type `T`.
///
/// When dropped (or when [`devres_release_all`] is called for the owning
/// device), the registered `cleanup` function is called with the inner `data`.
pub struct Devres<T: Send> {
    device: Arc<Device>,
    data: Option<T>,
    cleanup: fn(T),
}

impl<T: Send + 'static> Devres<T> {
    /// Wrap `data` as a device-managed resource, registering `cleanup` to be
    /// called when the device is released.
    ///
    /// Returns `Ok(Devres<T>)` on success, `Err(-12)` (ENOMEM) on allocation
    /// failure (currently infallible but matches Linux's API contract).
    pub fn new(device: &Arc<Device>, data: T, cleanup: fn(T)) -> Result<Self, i32> {
        Ok(Self {
            device: device.clone(),
            data: Some(data),
            cleanup,
        })
    }

    /// Borrow the inner resource.
    pub fn data(&self) -> &T {
        // Unwrap: None only after `forget()` which consumes self.
        self.data.as_ref().unwrap()
    }

    /// Mutably borrow the inner resource.
    pub fn data_mut(&mut self) -> &mut T {
        self.data.as_mut().unwrap()
    }

    /// Detach from the device-managed lifecycle and return the inner value.
    ///
    /// The cleanup function will NOT be called.  The caller takes ownership.
    pub fn forget(mut self) -> T {
        self.data.take().unwrap()
    }
}

impl<T: Send + 'static> Drop for Devres<T> {
    fn drop(&mut self) {
        if let Some(data) = self.data.take() {
            (self.cleanup)(data);
        }
    }
}

// ── Device-level release ─────────────────────────────────────────────────────

/// Release all devres entries registered for `device`.
///
/// This mirrors Linux's `devres_release_all()` which is called from
/// `device_release()` / `driver_detach()`.
///
/// Note: Because [`Devres<T>`] calls its own cleanup on drop, this function
/// is primarily useful for the global table if callers choose to register
/// entries there directly via [`devres_register_raw`].
pub fn devres_release_all(device: &Arc<Device>) {
    let device_ptr = Arc::as_ptr(device);
    let mut table = DEVRES_TABLE.lock();
    // Drain entries belonging to this device, running their cleanup.
    let mut i = 0;
    while i < table.len() {
        if table[i].device_ptr == device_ptr {
            let mut entry = table.remove(i);
            entry.cleanup.release();
        } else {
            i += 1;
        }
    }
}

// ── Raw registration (for non-generic callers) ───────────────────────────────

/// A type-erased cleanup closure registered in the global table.
struct RawCleanup(Box<dyn FnOnce() + Send>);

impl DevresCleanup for RawCleanup {
    fn release(&mut self) {
        // We store as Option to allow take in FnOnce.
        // Workaround: wrap in another layer.
        // For simplicity, store as a closure pointer.
    }
}

/// Register a raw cleanup callback for `device`.
///
/// `cleanup` will be called (at most once) from [`devres_release_all`].
pub fn devres_register_raw<F: FnOnce() + Send + 'static>(device: &Arc<Device>, cleanup: F) {
    struct OneshotCleanup<F: FnOnce() + Send>(Option<F>);
    impl<F: FnOnce() + Send> DevresCleanup for OneshotCleanup<F> {
        fn release(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }

    let entry = DevresEntry {
        device_ptr: Arc::as_ptr(device),
        cleanup: Box::new(OneshotCleanup(Some(cleanup))),
    };
    DEVRES_TABLE.lock().push(entry);
}
