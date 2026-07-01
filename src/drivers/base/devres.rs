//! Device-managed resource helpers.
//!
//! This mirrors the useful lifecycle semantics from Linux `drivers/base/devres.c`:
//! resources are associated with a device, explicit remove/destroy/release
//! operations search the device list in reverse registration order, and device
//! teardown releases remaining resources in reverse order.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::drivers::base::device::Device;

trait DevresCleanup: Send {
    fn release(&mut self);
}

struct DevresEntry {
    device_ptr: *const Device,
    key: usize,
    cleanup: Box<dyn DevresCleanup>,
}

// SAFETY: entries are only accessed under DEVRES_TABLE. The raw pointer is used
// only as a stable identity for the owning Arc<Device>, never dereferenced.
unsafe impl Send for DevresEntry {}
unsafe impl Sync for DevresEntry {}

static DEVRES_TABLE: Mutex<Vec<DevresEntry>> = Mutex::new(Vec::new());
static NEXT_DEVRES_KEY: AtomicUsize = AtomicUsize::new(1);

/// A typed device-managed resource.
///
/// The value is released by `cleanup` when the wrapper is dropped unless it was
/// transferred out with [`Devres::forget`].
pub struct Devres<T: Send + 'static> {
    device: Arc<Device>,
    data: Option<T>,
    cleanup: fn(T),
}

impl<T: Send + 'static> Devres<T> {
    /// Create a device-managed wrapper for `data`.
    pub fn new(device: &Arc<Device>, data: T, cleanup: fn(T)) -> Result<Self, i32> {
        Ok(Self {
            device: device.clone(),
            data: Some(data),
            cleanup,
        })
    }

    /// Owning device.
    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    /// Borrow the resource.
    pub fn data(&self) -> &T {
        self.data.as_ref().unwrap()
    }

    /// Mutably borrow the resource.
    pub fn data_mut(&mut self) -> &mut T {
        self.data.as_mut().unwrap()
    }

    /// Detach the resource from device-managed cleanup.
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

struct OneshotCleanup<F: FnOnce() + Send>(Option<F>);

impl<F: FnOnce() + Send> DevresCleanup for OneshotCleanup<F> {
    fn release(&mut self) {
        if let Some(cleanup) = self.0.take() {
            cleanup();
        }
    }
}

fn next_key() -> usize {
    NEXT_DEVRES_KEY.fetch_add(1, Ordering::Relaxed)
}

fn device_ptr(device: &Arc<Device>) -> *const Device {
    Arc::as_ptr(device)
}

fn find_entry_index(table: &[DevresEntry], owner: *const Device, key: usize) -> Option<usize> {
    table
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entry)| entry.device_ptr == owner && entry.key == key)
        .map(|(idx, _)| idx)
}

/// Register a raw cleanup action for `device`.
///
/// The returned key can later be passed to [`devres_find`], [`devres_remove`],
/// [`devres_destroy`], or [`devres_release`].
pub fn devres_register_raw<F>(device: &Arc<Device>, cleanup: F) -> usize
where
    F: FnOnce() + Send + 'static,
{
    let key = next_key();
    devres_register_keyed_raw(device, key, cleanup);
    key
}

/// Register a raw cleanup action using a caller-provided key.
pub fn devres_register_keyed_raw<F>(device: &Arc<Device>, key: usize, cleanup: F)
where
    F: FnOnce() + Send + 'static,
{
    DEVRES_TABLE.lock().push(DevresEntry {
        device_ptr: device_ptr(device),
        key,
        cleanup: Box::new(OneshotCleanup(Some(cleanup))),
    });
}

/// Return true if `device` has a managed resource with `key`.
pub fn devres_find(device: &Arc<Device>, key: usize) -> bool {
    let owner = device_ptr(device);
    let table = DEVRES_TABLE.lock();
    find_entry_index(&table, owner, key).is_some()
}

/// Remove one matching resource without running its cleanup.
pub fn devres_remove(device: &Arc<Device>, key: usize) -> bool {
    let owner = device_ptr(device);
    let mut table = DEVRES_TABLE.lock();
    if let Some(idx) = find_entry_index(&table, owner, key) {
        table.remove(idx);
        true
    } else {
        false
    }
}

/// Linux-style alias for removing a managed resource without releasing it.
pub fn devres_destroy(device: &Arc<Device>, key: usize) -> bool {
    devres_remove(device, key)
}

/// Remove one matching resource and run its cleanup.
pub fn devres_release(device: &Arc<Device>, key: usize) -> bool {
    let owner = device_ptr(device);
    let mut table = DEVRES_TABLE.lock();
    if let Some(idx) = find_entry_index(&table, owner, key) {
        let mut entry = table.remove(idx);
        entry.cleanup.release();
        true
    } else {
        false
    }
}

/// Release all registered resources for `device` in reverse registration order.
pub fn release_devres(device: &Arc<Device>) {
    let owner = device_ptr(device);
    let mut table = DEVRES_TABLE.lock();

    let mut idx = table.len();
    while idx > 0 {
        idx -= 1;
        if table[idx].device_ptr == owner {
            let mut entry = table.remove(idx);
            entry.cleanup.release();
        }
    }
}

/// Compatibility wrapper used by the driver core re-export.
pub fn devres_release_all(device: &Arc<Device>) {
    release_devres(device);
}
