//! Keyed data lists matching `gdataset.h` / `gdataset.c`.
//!
//! [`DataList`] stores arbitrary pointer values keyed by [`Quark`] identifiers.
//! Destroy notifications run when entries are replaced, removed, or when the
//! list is cleared via [`datalist_clear`].
//!
//! ## Location-associated datasets (future FFI)
//!
//! GLib's `g_dataset_*` API attaches keyed lists to arbitrary memory locations
//! through a process-global hash table (`g_dataset_location_ht`). That layer
//! depends on stable C pointer identities and will be exposed via `extern "C"`
//! once the quark subsystem and FFI wiring land. This module implements the
//! standalone datalist API used directly by Rust callers.

use crate::prelude::*;
use crate::quark::Quark;
use alloc::collections::BTreeMap;
use core::cell::RefCell;
use core::ffi::c_void;
use core::ptr;

/// Opaque pointer payload (`gpointer`).
pub type GPointer = *mut c_void;

/// Foreach callback (`GDataForeachFunc`).
pub type DataForeachFunc = fn(Quark, GPointer, *mut c_void);

/// Element destructor (`GDestroyNotify`).
pub type DestroyNotify = Box<dyn FnMut(GPointer)>;

const INDEX_THRESHOLD: usize = 33;

struct DataElt {
    key: Quark,
    data: GPointer,
    destroy: Option<DestroyNotify>,
}

struct DataListStorage {
    entries: Vec<DataElt>,
    index: Option<BTreeMap<Quark, usize>>,
}

/// Keyed data list (`GData`).
///
/// Wraps quark-keyed pointer storage. Use [`datalist_init`] on a fresh value
/// and [`datalist_clear`] to free all entries and invoke destroy callbacks.
#[derive(Default)]
pub struct DataList {
    storage: RefCell<Option<Box<DataListStorage>>>,
}

impl DataList {
    /// Create an empty datalist (`g_datalist_init` on a fresh pointer).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the list contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage
            .borrow()
            .as_ref()
            .is_none_or(|s| s.entries.is_empty())
    }

    /// Number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.storage
            .borrow()
            .as_ref()
            .map_or(0, |s| s.entries.len())
    }
}

/// Reset a datalist to empty without calling destroy functions (`g_datalist_init`).
///
/// Does not invoke destroy callbacks; any previous storage is dropped without
/// notification, matching GLib's C behavior for re-initialization.
pub fn datalist_init(datalist: &DataList) {
    *datalist.storage.borrow_mut() = None;
}

/// Free all elements and invoke destroy callbacks (`g_datalist_clear`).
pub fn datalist_clear(datalist: &DataList) {
    let Some(storage) = datalist.storage.borrow_mut().take() else {
        return;
    };

    for entry in storage.entries {
        if let Some(mut destroy) = entry.destroy {
            destroy(entry.data);
        }
    }
}

/// Retrieve the data element for `key_id` (`g_datalist_id_get_data`).
#[must_use]
pub fn datalist_id_get_data(datalist: &DataList, key_id: Quark) -> GPointer {
    if key_id == 0 {
        return ptr::null_mut();
    }

    let storage = datalist.storage.borrow();
    let Some(storage) = &*storage else {
        return ptr::null_mut();
    };

    find_index(storage, key_id)
        .map(|idx| storage.entries[idx].data)
        .unwrap_or(ptr::null_mut())
}

/// Set or remove the data element for `key_id` (`g_datalist_id_set_data_full`).
///
/// Pass a null `data` pointer to remove the entry; `destroy` must be `None` when
/// removing. When replacing an existing value, any previous destroy callback runs
/// after the entry is updated.
pub fn datalist_id_set_data_full(
    datalist: &DataList,
    key_id: Quark,
    data: GPointer,
    destroy: Option<DestroyNotify>,
) {
    if data.is_null() {
        debug_assert!(destroy.is_none(), "destroy must be None when removing data");
        if key_id == 0 {
            return;
        }
        let _ = data_set_internal(datalist, key_id, None, false);
        return;
    }

    debug_assert!(key_id != 0, "key_id must be non-zero when setting data");
    if key_id == 0 {
        return;
    }

    let _ = data_set_internal(
        datalist,
        key_id,
        Some(DataAssignment { data, destroy }),
        false,
    );
}

/// Remove an element without calling its destroy notification
/// (`g_datalist_id_remove_no_notify`).
#[must_use]
pub fn datalist_id_remove_no_notify(datalist: &DataList, key_id: Quark) -> GPointer {
    if key_id == 0 {
        return ptr::null_mut();
    }

    data_set_internal(datalist, key_id, None, true).unwrap_or(ptr::null_mut())
}

/// Call `func` for each element (`g_datalist_foreach`).
///
/// Not thread-safe: callers must exclude concurrent modification while iterating.
/// Mutations from `func` may not be reflected beyond skipped removed keys, matching
/// GLib semantics.
pub fn datalist_foreach<F>(datalist: &DataList, mut func: F)
where
    F: FnMut(Quark, GPointer),
{
    let keys: Vec<Quark> = {
        let storage = datalist.storage.borrow();
        let Some(storage) = &*storage else {
            return;
        };
        storage.entries.iter().map(|entry| entry.key).collect()
    };

    for key in keys {
        let element = {
            let storage = datalist.storage.borrow();
            let Some(storage) = &*storage else {
                break;
            };
            find_index(storage, key).map(|idx| {
                let entry = &storage.entries[idx];
                (entry.key, entry.data)
            })
        };

        if let Some((entry_key, entry_data)) = element {
            func(entry_key, entry_data);
        }
    }
}

/// Convenience wrapper for [`datalist_id_set_data_full`] without a destroy callback.
pub fn datalist_id_set_data(datalist: &DataList, key_id: Quark, data: GPointer) {
    datalist_id_set_data_full(datalist, key_id, data, None);
}

struct DataAssignment {
    data: GPointer,
    destroy: Option<DestroyNotify>,
}

fn find_index(storage: &DataListStorage, key_id: Quark) -> Option<usize> {
    if let Some(index) = &storage.index {
        return index.get(&key_id).copied();
    }

    storage.entries.iter().position(|entry| entry.key == key_id)
}

fn maybe_enable_index(storage: &mut DataListStorage) {
    if storage.entries.len() >= INDEX_THRESHOLD && storage.index.is_none() {
        let mut index = BTreeMap::new();
        for (idx, entry) in storage.entries.iter().enumerate() {
            index.insert(entry.key, idx);
        }
        storage.index = Some(index);
    }
}

fn maybe_disable_index(storage: &mut DataListStorage) {
    if storage.entries.len() <= INDEX_THRESHOLD / 2 {
        storage.index = None;
    }
}

fn remove_at(storage: &mut DataListStorage, idx: usize) {
    debug_assert!(idx < storage.entries.len());

    if let Some(index) = storage.index.as_mut() {
        index.remove(&storage.entries[idx].key);
    }

    let last = storage.entries.len() - 1;
    storage.entries.swap(idx, last);
    storage.entries.pop();

    if idx < storage.entries.len() {
        if let Some(index) = storage.index.as_mut() {
            index.insert(storage.entries[idx].key, idx);
        }
    }

    maybe_disable_index(storage);
}

fn data_set_internal(
    datalist: &DataList,
    key_id: Quark,
    assignment: Option<DataAssignment>,
    steal: bool,
) -> Option<GPointer> {
    let mut storage_slot = datalist.storage.borrow_mut();

    match assignment {
        None => {
            let storage = storage_slot.as_mut()?;
            let idx = find_index(storage, key_id)?;
            let old_data = storage.entries[idx].data;
            let old_destroy = storage.entries[idx].destroy.take();

            remove_at(storage, idx);

            if storage.entries.is_empty() {
                *storage_slot = None;
            }

            drop(storage_slot);

            if old_destroy.is_some() && !steal {
                if let Some(mut destroy) = old_destroy {
                    destroy(old_data);
                }
            }

            Some(old_data)
        }
        Some(assignment) => {
            if let Some(storage) = storage_slot.as_mut() {
                if let Some(idx) = find_index(storage, key_id) {
                    let entry = &mut storage.entries[idx];
                    if entry.destroy.is_none() {
                        entry.data = assignment.data;
                        entry.destroy = assignment.destroy;
                        return None;
                    }

                    let old_data = entry.data;
                    let old_destroy = entry.destroy.take();
                    entry.data = assignment.data;
                    entry.destroy = assignment.destroy;

                    drop(storage_slot);

                    if let Some(mut destroy) = old_destroy {
                        destroy(old_data);
                    }
                    return None;
                }
            }

            let storage = storage_slot.get_or_insert_with(|| {
                Box::new(DataListStorage {
                    entries: Vec::with_capacity(2),
                    index: None,
                })
            });

            storage.entries.push(DataElt {
                key: key_id,
                data: assignment.data,
                destroy: assignment.destroy,
            });
            let idx = storage.entries.len() - 1;
            if let Some(index) = storage.index.as_mut() {
                index.insert(key_id, idx);
            } else {
                maybe_enable_index(storage);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn str_ptr(s: &'static str) -> GPointer {
        s.as_ptr().cast_mut().cast()
    }

    fn int_ptr(value: i32) -> GPointer {
        value as isize as GPointer
    }

    fn ptr_to_int(value: GPointer) -> i32 {
        value as isize as i32
    }

    #[test]
    fn datalist_init_empty() {
        let list = DataList::new();
        datalist_init(&list);
        assert!(list.is_empty());
        assert!(datalist_id_get_data(&list, 1).is_null());
    }

    #[test]
    fn datalist_basic_few() {
        const BOGUS_QUARK: Quark = 1_000_000_000;
        let list = DataList::new();
        let data = str_ptr("one");

        datalist_init(&list);
        datalist_id_set_data(&list, BOGUS_QUARK, data);

        let ret = datalist_id_get_data(&list, BOGUS_QUARK);
        assert_eq!(ret, data);

        assert!(datalist_id_get_data(&list, BOGUS_QUARK + 1).is_null());
        assert!(datalist_id_get_data(&list, 0).is_null());

        datalist_clear(&list);
        assert!(list.is_empty());
    }

    #[test]
    fn datalist_basic_many_uses_index_path() {
        const BOGUS_QUARK: Quark = 1_000_000_000;
        let list = DataList::new();
        let data = str_ptr("one");

        datalist_init(&list);

        for i in 0..200 {
            datalist_id_set_data(&list, BOGUS_QUARK + 1 + i, data);
        }

        assert!(list.len() >= INDEX_THRESHOLD);
        datalist_id_set_data(&list, BOGUS_QUARK, data);

        let ret = datalist_id_get_data(&list, BOGUS_QUARK);
        assert_eq!(ret, data);

        datalist_clear(&list);
        assert!(list.is_empty());
    }

    #[test]
    fn datalist_id_set_get_remove() {
        let list = DataList::new();
        let data = str_ptr("one");
        let one = 42_u32;
        let two = 99_u32;

        datalist_init(&list);
        datalist_id_set_data(&list, one, data);

        let ret = datalist_id_get_data(&list, one);
        assert_eq!(ret, data);

        assert!(datalist_id_get_data(&list, two).is_null());
        assert!(datalist_id_get_data(&list, 0).is_null());

        datalist_id_set_data(&list, one, str_ptr("new-value"));
        let ret = datalist_id_get_data(&list, one);
        assert_ne!(ret, data);

        datalist_id_set_data(&list, one, ptr::null_mut());
        assert!(datalist_id_get_data(&list, one).is_null());

        datalist_clear(&list);
    }

    #[test]
    fn datalist_clear_recursive_from_destroy() {
        thread_local! {
            static GLOBAL_LIST: DataList = DataList::new();
        }

        GLOBAL_LIST.with(|list| {
            datalist_init(list);

            let destroy_one = Box::new(|_: GPointer| {
                GLOBAL_LIST.with(datalist_clear);
            });

            datalist_id_set_data_full(list, 1, int_ptr(1), Some(destroy_one));
            datalist_id_set_data(list, 2, int_ptr(2));

            datalist_clear(list);
            assert!(list.is_empty());
        });
    }

    #[test]
    fn datalist_destroy_on_remove_and_replace() {
        let counter = Arc::new(AtomicU32::new(0));
        let list = DataList::new();

        let notify = {
            let counter = Arc::clone(&counter);
            Box::new(move |_: GPointer| {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        datalist_id_set_data_full(&list, 1, str_ptr("test1"), Some(notify));

        counter.store(0, Ordering::SeqCst);
        datalist_id_set_data(&list, 1, ptr::null_mut());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let notify2 = {
            let counter = Arc::clone(&counter);
            Box::new(move |_: GPointer| {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };
        datalist_id_set_data_full(&list, 1, str_ptr("test1"), Some(notify2));

        counter.store(0, Ordering::SeqCst);
        datalist_id_set_data(&list, 1, ptr::null_mut());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        datalist_clear(&list);
    }

    #[test]
    fn datalist_remove_no_notify_skips_destroy() {
        let counter = Arc::new(AtomicU32::new(0));
        let list = DataList::new();

        let notify = {
            let counter = Arc::clone(&counter);
            Box::new(move |_: GPointer| {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        datalist_id_set_data_full(&list, 1, str_ptr("test1"), Some(notify));

        counter.store(0, Ordering::SeqCst);
        let stolen = datalist_id_remove_no_notify(&list, 1);
        assert_eq!(stolen, str_ptr("test1"));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        assert!(datalist_id_get_data(&list, 1).is_null());
    }

    #[test]
    fn datalist_foreach_visits_all_entries() {
        let counter = Arc::new(AtomicU32::new(0));
        let list = DataList::new();

        let notify = {
            let counter = Arc::clone(&counter);
            Box::new(move |_: GPointer| {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        datalist_id_set_data_full(&list, 1, int_ptr(1), Some(notify));
        datalist_id_set_data(&list, 2, int_ptr(2));
        datalist_id_set_data(&list, 3, int_ptr(3));

        let mut count = 0_u32;
        datalist_foreach(&list, |_, _| count += 1);
        assert_eq!(count, 3);

        datalist_clear(&list);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn datalist_foreach_skips_removed_during_iteration() {
        let list = DataList::new();
        datalist_id_set_data(&list, 1, int_ptr(1));
        datalist_id_set_data(&list, 2, int_ptr(2));
        datalist_id_set_data(&list, 3, int_ptr(3));

        let mut seen = Vec::new();
        datalist_foreach(&list, |key, data| {
            seen.push((key, ptr_to_int(data)));
            if key == 2 {
                datalist_id_set_data(&list, 3, ptr::null_mut());
            }
        });

        assert_eq!(seen.len(), 2);
        assert!(datalist_id_get_data(&list, 3).is_null());
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn datalist_clear_invokes_all_destroy_callbacks() {
        let counter = Arc::new(AtomicU32::new(0));
        let list = DataList::new();

        for key in 1..=3 {
            let counter = Arc::clone(&counter);
            let notify = Box::new(move |_: GPointer| {
                counter.fetch_add(1, Ordering::SeqCst);
            });
            datalist_id_set_data_full(&list, key, int_ptr(key as i32), Some(notify));
        }

        datalist_clear(&list);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
        assert!(list.is_empty());
    }
}
