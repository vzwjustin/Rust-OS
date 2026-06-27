//! Hash table matching `ghash.h` / `ghash.c`.
//!
//! Open-addressing table with prime-modulo bucketing, tombstones, and set
//! storage (shared key/value arrays when every value equals its key).

use crate::prelude::*;
use crate::ptr_array::GPointer;
use crate::refcount::AtomicRefCount;
use core::cell::RefCell;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;

const HASH_TABLE_MIN_SHIFT: u32 = 3;
const UNUSED_HASH_VALUE: u32 = 0;
const TOMBSTONE_HASH_VALUE: u32 = 1;
const ITER_POSITION_INVALID: u32 = u32::MAX;

/// Prime moduli for power-of-two table sizes (from `ghash.c`).
const PRIME_MOD: [u32; 32] = [
    1, 2, 3, 7, 13, 31, 61, 127, 251, 509, 1021, 2039, 4093, 8191, 16381, 32749, 65521, 131071,
    262139, 524287, 1048573, 2097143, 4194301, 8388593, 16777213, 33554393, 67108859, 134217689,
    268435399, 536870909, 1073741789, 2147483647,
];

/// Hash function (`GHashFunc`).
pub type HashFunc = fn(*const ()) -> u32;

/// Key equality function (`GEqualFunc`). When `None`, keys compare by address.
pub type EqualFunc = fn(*const (), *const ()) -> bool;

type KeyDestroyFn = Option<Box<dyn FnMut(GPointer)>>;
type ValueDestroyFn = Option<Box<dyn FnMut(GPointer)>>;

struct HashTableStorage {
    bucket_count: u32,
    mod_: u32,
    mask: u32,
    nnodes: u32,
    noccupied: u32,
    is_set: bool,
    keys: Vec<GPointer>,
    hashes: Vec<u32>,
    values: Vec<GPointer>,
    version: u64,
}

struct InsertParams {
    node_index: u32,
    key_hash: u32,
    new_key: GPointer,
    new_value: GPointer,
    keep_new_key: bool,
    reusing_key: bool,
}

struct HashTableInner {
    ref_count: AtomicRefCount,
    hash_func: HashFunc,
    key_equal_func: Option<EqualFunc>,
    key_destroy_func: RefCell<KeyDestroyFn>,
    value_destroy_func: RefCell<ValueDestroyFn>,
    storage: RefCell<HashTableStorage>,
}

/// Reference-counted hash table (`GHashTable`).
pub struct HashTable {
    inner: NonNull<HashTableInner>,
}

/// Hash-table iterator (`GHashTableIter`).
pub struct HashTableIter<'table> {
    table: &'table HashTable,
    position: u32,
    version: u64,
}

fn hash_is_real(h: u32) -> bool {
    h >= 2
}

fn normalize_hash(hash: u32) -> u32 {
    if hash < 2 {
        2
    } else {
        hash
    }
}

fn find_closest_shift(mut n: u32) -> u32 {
    let mut shift = 0;
    while n != 0 {
        shift += 1;
        n >>= 1;
    }
    shift
}

fn set_shift(storage: &mut HashTableStorage, shift: u32) {
    assert!(
        shift <= 31,
        "adding more entries to hash table would overflow"
    );
    let bucket_count = 1u32 << shift;
    storage.bucket_count = bucket_count;
    storage.mod_ = PRIME_MOD[shift as usize];
    debug_assert_eq!(bucket_count & (bucket_count - 1), 0);
    storage.mask = bucket_count - 1;
}

fn set_shift_from_size(storage: &mut HashTableStorage, target: u32) {
    let shift = find_closest_shift(target).max(HASH_TABLE_MIN_SHIFT);
    set_shift(storage, shift);
}

fn hash_to_index(mod_: u32, hash: u32) -> u32 {
    ((u64::from(hash) * 11) % u64::from(mod_)) as u32
}

fn setup_storage(storage: &mut HashTableStorage) {
    set_shift(storage, HASH_TABLE_MIN_SHIFT);
    storage.nnodes = 0;
    storage.noccupied = 0;
    storage.is_set = true;
    storage.keys = vec![core::ptr::null_mut(); storage.bucket_count as usize];
    storage.values = Vec::new();
    storage.hashes = vec![UNUSED_HASH_VALUE; storage.bucket_count as usize];
}

fn keys_equal(
    key_equal_func: Option<EqualFunc>,
    node_key: GPointer,
    lookup_key: *const (),
) -> bool {
    if let Some(eq) = key_equal_func {
        eq(node_key.cast_const(), lookup_key)
    } else {
        node_key.cast_const() == lookup_key
    }
}

fn lookup_node(
    storage: &HashTableStorage,
    hash_func: HashFunc,
    key_equal_func: Option<EqualFunc>,
    key: *const (),
) -> (u32, u32) {
    let hash_value = normalize_hash(hash_func(key));
    let mut node_index = hash_to_index(storage.mod_, hash_value);
    let mut node_hash = storage.hashes[node_index as usize];
    let mut first_tombstone = 0u32;
    let mut have_tombstone = false;
    let mut step = 0u32;

    while !matches!(node_hash, UNUSED_HASH_VALUE) {
        if node_hash == hash_value {
            let node_key = storage.keys[node_index as usize];
            if keys_equal(key_equal_func, node_key, key) {
                return (node_index, hash_value);
            }
        } else if node_hash == TOMBSTONE_HASH_VALUE && !have_tombstone {
            first_tombstone = node_index;
            have_tombstone = true;
        }

        step += 1;
        node_index = (node_index + step) & storage.mask;
        node_hash = storage.hashes[node_index as usize];
    }

    if have_tombstone {
        (first_tombstone, hash_value)
    } else {
        (node_index, hash_value)
    }
}

fn fetch_value(storage: &HashTableStorage, index: u32) -> GPointer {
    if storage.is_set {
        storage.keys[index as usize]
    } else {
        storage.values[index as usize]
    }
}

fn ensure_keyval_fits(storage: &mut HashTableStorage, key: GPointer, value: GPointer) {
    if storage.is_set && key != value {
        storage.values = storage.keys.clone();
        storage.is_set = false;
    }
}

fn maybe_resize(storage: &mut HashTableStorage) {
    let noccupied = storage.noccupied;
    let bucket_count = storage.bucket_count;
    if (bucket_count > (1 << HASH_TABLE_MIN_SHIFT) && (bucket_count - 1) / 4 >= storage.nnodes)
        || bucket_count <= noccupied + noccupied / 16
    {
        resize(storage);
    }
}

fn resize(storage: &mut HashTableStorage) {
    let old_size = storage.bucket_count;
    set_shift_from_size(storage, storage.nnodes + storage.nnodes / 3);

    if storage.bucket_count > old_size {
        storage
            .keys
            .resize(storage.bucket_count as usize, core::ptr::null_mut());
        storage
            .hashes
            .resize(storage.bucket_count as usize, UNUSED_HASH_VALUE);
        if !storage.is_set {
            storage
                .values
                .resize(storage.bucket_count as usize, core::ptr::null_mut());
        }
    }

    let bitmap_len = (storage.bucket_count as usize).div_ceil(32);
    let mut reallocated = vec![0u32; bitmap_len.max((old_size as usize).div_ceil(32))];

    for i in 0..old_size {
        let node_hash = storage.hashes[i as usize];
        if !hash_is_real(node_hash) {
            storage.hashes[i as usize] = UNUSED_HASH_VALUE;
            continue;
        }

        if get_status_bit(&reallocated, i) {
            continue;
        }

        storage.hashes[i as usize] = UNUSED_HASH_VALUE;
        let key = storage.keys[i as usize];
        let value = fetch_value(storage, i);
        storage.keys[i as usize] = core::ptr::null_mut();
        if !storage.is_set {
            storage.values[i as usize] = core::ptr::null_mut();
        }

        let mut node_hash = node_hash;
        let mut evicted_key = key;
        let mut evicted_value = value;

        loop {
            let mut hash_val = hash_to_index(storage.mod_, node_hash);
            let mut step = 0u32;
            while get_status_bit(&reallocated, hash_val) {
                step += 1;
                hash_val = (hash_val + step) & storage.mask;
            }
            set_status_bit(&mut reallocated, hash_val);

            let replaced_hash = storage.hashes[hash_val as usize];
            storage.hashes[hash_val as usize] = node_hash;

            if !hash_is_real(replaced_hash) {
                storage.keys[hash_val as usize] = evicted_key;
                if storage.is_set {
                    // value lives in keys
                } else {
                    storage.values[hash_val as usize] = evicted_value;
                }
                break;
            }

            node_hash = replaced_hash;
            let prev_key = storage.keys[hash_val as usize];
            let prev_value = fetch_value(storage, hash_val);
            storage.keys[hash_val as usize] = evicted_key;
            if storage.is_set {
                evicted_key = prev_key;
            } else {
                storage.values[hash_val as usize] = evicted_value;
                evicted_key = prev_key;
                evicted_value = prev_value;
            }
        }
    }

    if storage.bucket_count < old_size {
        storage.keys.truncate(storage.bucket_count as usize);
        storage.hashes.truncate(storage.bucket_count as usize);
        if !storage.is_set {
            storage.values.truncate(storage.bucket_count as usize);
        }
    }

    storage.noccupied = storage.nnodes;
}

fn get_status_bit(bitmap: &[u32], index: u32) -> bool {
    ((bitmap[index as usize / 32] >> (index % 32)) & 1) == 1
}

fn set_status_bit(bitmap: &mut [u32], index: u32) {
    bitmap[index as usize / 32] |= 1u32 << (index % 32);
}

fn bump_version(storage: &mut HashTableStorage) {
    storage.version = storage.version.wrapping_add(1);
}

fn call_key_destroy(inner: &HashTableInner, key: GPointer) {
    if let Some(ref mut f) = *inner.key_destroy_func.borrow_mut() {
        f(key);
    }
}

fn call_value_destroy(inner: &HashTableInner, value: GPointer) {
    if let Some(ref mut f) = *inner.value_destroy_func.borrow_mut() {
        f(value);
    }
}

fn remove_node(inner: &HashTableInner, storage: &mut HashTableStorage, index: u32, notify: bool) {
    let key = storage.keys[index as usize];
    let value = fetch_value(storage, index);

    storage.hashes[index as usize] = TOMBSTONE_HASH_VALUE;
    storage.keys[index as usize] = core::ptr::null_mut();
    if !storage.is_set {
        storage.values[index as usize] = core::ptr::null_mut();
    }

    debug_assert!(storage.nnodes > 0);
    storage.nnodes -= 1;

    if notify {
        call_key_destroy(inner, key);
        call_value_destroy(inner, value);
    }
}

impl HashTable {
    /// Create a hash table (`g_hash_table_new`).
    pub fn new(hash_func: Option<HashFunc>, key_equal_func: Option<EqualFunc>) -> Self {
        Self::new_full(hash_func, key_equal_func, None, None)
    }

    /// Create a hash table with destroy notifies (`g_hash_table_new_full`).
    pub fn new_full(
        hash_func: Option<HashFunc>,
        key_equal_func: Option<EqualFunc>,
        key_destroy_func: Option<Box<dyn FnMut(GPointer)>>,
        value_destroy_func: Option<Box<dyn FnMut(GPointer)>>,
    ) -> Self {
        let mut storage = HashTableStorage {
            bucket_count: 0,
            mod_: 0,
            mask: 0,
            nnodes: 0,
            noccupied: 0,
            is_set: true,
            keys: Vec::new(),
            hashes: Vec::new(),
            values: Vec::new(),
            version: 0,
        };
        setup_storage(&mut storage);

        let inner = Box::new(HashTableInner {
            ref_count: AtomicRefCount::new(),
            hash_func: hash_func.unwrap_or(direct_hash),
            key_equal_func,
            key_destroy_func: RefCell::new(key_destroy_func),
            value_destroy_func: RefCell::new(value_destroy_func),
            storage: RefCell::new(storage),
        });

        Self {
            inner: NonNull::new(Box::into_raw(inner)).expect("Box::into_raw returned null"),
        }
    }

    /// Destroy the table, freeing keys and values (`g_hash_table_destroy`).
    pub fn destroy(self) {
        self.remove_all();
        self.unref();
    }

    /// Insert a key/value pair (`g_hash_table_insert`).
    ///
    /// Returns `true` when the key was not already present.
    pub fn insert(&self, key: GPointer, value: GPointer) -> bool {
        self.insert_internal(key, value, false)
    }

    /// Replace or insert a key/value pair (`g_hash_table_replace`).
    ///
    /// Returns `true` when the key was not already present.
    pub fn replace(&self, key: GPointer, value: GPointer) -> bool {
        self.insert_internal(key, value, true)
    }

    /// Insert a key into a set (`g_hash_table_add`).
    ///
    /// Returns `true` when the key was not already present.
    pub fn add(&self, key: GPointer) -> bool {
        self.insert_internal(key, key, true)
    }

    /// Look up a value by key (`g_hash_table_lookup`).
    pub fn lookup(&self, key: *const ()) -> GPointer {
        let storage = self.inner().storage.borrow();
        let (node_index, _) = lookup_node(
            &storage,
            self.inner().hash_func,
            self.inner().key_equal_func,
            key,
        );
        if hash_is_real(storage.hashes[node_index as usize]) {
            fetch_value(&storage, node_index)
        } else {
            core::ptr::null_mut()
        }
    }

    /// Whether `key` is present (`g_hash_table_contains`).
    pub fn contains(&self, key: *const ()) -> bool {
        let storage = self.inner().storage.borrow();
        let (node_index, _) = lookup_node(
            &storage,
            self.inner().hash_func,
            self.inner().key_equal_func,
            key,
        );
        hash_is_real(storage.hashes[node_index as usize])
    }

    /// Look up a key, returning the canonical table key and value
    /// (`g_hash_table_lookup_extended`).
    pub fn lookup_extended(
        &self,
        lookup_key: *const (),
        orig_key: Option<&mut GPointer>,
        value: Option<&mut GPointer>,
    ) -> bool {
        let storage = self.inner().storage.borrow();
        let (node_index, _) = lookup_node(
            &storage,
            self.inner().hash_func,
            self.inner().key_equal_func,
            lookup_key,
        );

        if !hash_is_real(storage.hashes[node_index as usize]) {
            if let Some(k) = orig_key {
                *k = core::ptr::null_mut();
            }
            if let Some(v) = value {
                *v = core::ptr::null_mut();
            }
            return false;
        }

        if let Some(k) = orig_key {
            *k = storage.keys[node_index as usize];
        }
        if let Some(v) = value {
            *v = fetch_value(&storage, node_index);
        }
        true
    }

    /// Remove a key, invoking destroy notifies (`g_hash_table_remove`).
    pub fn remove(&self, key: *const ()) -> bool {
        self.remove_internal(key, true)
    }

    /// Remove a key without destroy notifies (`g_hash_table_steal`).
    pub fn steal(&self, key: *const ()) -> bool {
        self.remove_internal(key, false)
    }

    /// Steal a key/value pair (`g_hash_table_steal_extended`).
    pub fn steal_extended(
        &self,
        lookup_key: *const (),
        stolen_key: Option<&mut GPointer>,
        stolen_value: Option<&mut GPointer>,
    ) -> bool {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        let (node_index, _) =
            lookup_node(&storage, inner.hash_func, inner.key_equal_func, lookup_key);

        if !hash_is_real(storage.hashes[node_index as usize]) {
            if let Some(k) = stolen_key {
                *k = core::ptr::null_mut();
            }
            if let Some(v) = stolen_value {
                *v = core::ptr::null_mut();
            }
            return false;
        }

        let key = storage.keys[node_index as usize];
        let value = fetch_value(&storage, node_index);

        let want_key = stolen_key.is_some();
        let want_value = stolen_value.is_some();

        if let Some(k) = stolen_key {
            *k = key;
            storage.keys[node_index as usize] = core::ptr::null_mut();
        }

        if let Some(v) = stolen_value {
            if want_key && storage.is_set {
                *v = key;
            } else {
                *v = value;
                if !storage.is_set {
                    storage.values[node_index as usize] = core::ptr::null_mut();
                }
            }
        }

        storage.hashes[node_index as usize] = TOMBSTONE_HASH_VALUE;
        if !want_key {
            storage.keys[node_index as usize] = core::ptr::null_mut();
        }
        if !want_value && !storage.is_set {
            storage.values[node_index as usize] = core::ptr::null_mut();
        }

        debug_assert!(storage.nnodes > 0);
        storage.nnodes -= 1;
        bump_version(&mut storage);
        maybe_resize(&mut storage);
        true
    }

    /// Remove every entry, calling destroy notifies (`g_hash_table_remove_all`).
    pub fn remove_all(&self) {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        if storage.nnodes != 0 {
            bump_version(&mut storage);
        }
        remove_all_nodes(inner, &mut storage, true, false);
        maybe_resize(&mut storage);
    }

    /// Remove every entry without destroy notifies (`g_hash_table_steal_all`).
    pub fn steal_all(&self) {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        if storage.nnodes != 0 {
            bump_version(&mut storage);
        }
        remove_all_nodes(inner, &mut storage, false, false);
        maybe_resize(&mut storage);
    }

    /// Call `func` for each key/value pair (`g_hash_table_foreach`).
    pub fn foreach<F>(&self, mut func: F)
    where
        F: FnMut(GPointer, GPointer),
    {
        let inner = self.inner();
        let storage = inner.storage.borrow();
        let version = storage.version;
        for i in 0..storage.bucket_count {
            let node_hash = storage.hashes[i as usize];
            if hash_is_real(node_hash) {
                let key = storage.keys[i as usize];
                let value = fetch_value(&storage, i);
                func(key, value);
                debug_assert_eq!(version, inner.storage.borrow().version);
            }
        }
    }

    /// Remove entries for which `func` returns true (`g_hash_table_foreach_remove`).
    pub fn foreach_remove<F>(&self, mut func: F) -> u32
    where
        F: FnMut(GPointer, GPointer) -> bool,
    {
        self.foreach_remove_or_steal(&mut func, true)
    }

    /// Remove entries without destroy notifies (`g_hash_table_foreach_steal`).
    pub fn foreach_steal<F>(&self, mut func: F) -> u32
    where
        F: FnMut(GPointer, GPointer) -> bool,
    {
        self.foreach_remove_or_steal(&mut func, false)
    }

    /// Find the first value matching `predicate` (`g_hash_table_find`).
    pub fn find<F>(&self, mut predicate: F) -> GPointer
    where
        F: FnMut(GPointer, GPointer) -> bool,
    {
        let inner = self.inner();
        let storage = inner.storage.borrow();
        let version = storage.version;
        for i in 0..storage.bucket_count {
            let node_hash = storage.hashes[i as usize];
            if hash_is_real(node_hash) {
                let key = storage.keys[i as usize];
                let value = fetch_value(&storage, i);
                if predicate(key, value) {
                    return value;
                }
                debug_assert_eq!(version, inner.storage.borrow().version);
            }
        }
        core::ptr::null_mut()
    }

    /// Number of entries (`g_hash_table_size`).
    pub fn size(&self) -> u32 {
        self.inner().storage.borrow().nnodes
    }

    /// Initialize an iterator (`g_hash_table_iter_init`).
    pub fn iter(&self) -> HashTableIter<'_> {
        HashTableIter {
            table: self,
            position: ITER_POSITION_INVALID,
            version: self.inner().storage.borrow().version,
        }
    }

    /// Increase the reference count (`g_hash_table_ref`).
    #[must_use]
    pub fn ref_(&self) -> Self {
        self.inner().ref_count.inc();
        Self { inner: self.inner }
    }

    /// Decrease the reference count (`g_hash_table_unref`).
    pub fn unref(self) {
        let this = ManuallyDrop::new(self);
        unsafe {
            if (*this.inner.as_ptr()).ref_count.dec() {
                release_hash_table(this.inner);
            }
        }
    }

    fn insert_internal(&self, key: GPointer, value: GPointer, keep_new_key: bool) -> bool {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        let (node_index, key_hash) = lookup_node(
            &storage,
            inner.hash_func,
            inner.key_equal_func,
            key.cast_const(),
        );
        insert_node(
            inner,
            &mut storage,
            InsertParams {
                node_index,
                key_hash,
                new_key: key,
                new_value: value,
                keep_new_key,
                reusing_key: false,
            },
        )
    }

    fn remove_internal(&self, key: *const (), notify: bool) -> bool {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        let (node_index, _) = lookup_node(&storage, inner.hash_func, inner.key_equal_func, key);
        if !hash_is_real(storage.hashes[node_index as usize]) {
            return false;
        }
        remove_node(inner, &mut storage, node_index, notify);
        bump_version(&mut storage);
        maybe_resize(&mut storage);
        true
    }

    fn foreach_remove_or_steal<F>(&self, func: &mut F, notify: bool) -> u32
    where
        F: FnMut(GPointer, GPointer) -> bool,
    {
        let inner = self.inner();
        let mut storage = inner.storage.borrow_mut();
        let version = storage.version;
        let mut deleted = 0u32;
        for i in 0..storage.bucket_count {
            let node_hash = storage.hashes[i as usize];
            if hash_is_real(node_hash) {
                let key = storage.keys[i as usize];
                let value = fetch_value(&storage, i);
                if func(key, value) {
                    remove_node(inner, &mut storage, i, notify);
                    deleted += 1;
                }
                debug_assert_eq!(version, storage.version);
            }
        }
        if deleted > 0 {
            bump_version(&mut storage);
        }
        maybe_resize(&mut storage);
        deleted
    }

    fn inner(&self) -> &HashTableInner {
        unsafe { self.inner.as_ref() }
    }
}

impl HashTableIter<'_> {
    /// Advance the iterator (`g_hash_table_iter_next`).
    pub fn next(&mut self, key: Option<&mut GPointer>, value: Option<&mut GPointer>) -> bool {
        let inner = self.table.inner();
        let storage = inner.storage.borrow();
        debug_assert_eq!(self.version, storage.version);
        debug_assert!(
            self.position < storage.bucket_count || self.position == ITER_POSITION_INVALID
        );

        let mut position = self.position;
        loop {
            position = position.wrapping_add(1);
            if position >= storage.bucket_count {
                self.position = position;
                return false;
            }
            if hash_is_real(storage.hashes[position as usize]) {
                break;
            }
        }

        if let Some(k) = key {
            *k = storage.keys[position as usize];
        }
        if let Some(v) = value {
            *v = fetch_value(&storage, position);
        }
        self.position = position;
        true
    }

    /// Return the associated hash table (`g_hash_table_iter_get_hash_table`).
    pub fn hash_table(&self) -> &HashTable {
        self.table
    }

    /// Remove the current entry with destroy notifies (`g_hash_table_iter_remove`).
    pub fn remove(&mut self) {
        self.remove_or_steal(true);
    }

    /// Replace the current entry's value (`g_hash_table_iter_replace`).
    pub fn replace(&mut self, value: GPointer) {
        let inner = self.table.inner();
        let mut storage = inner.storage.borrow_mut();
        debug_assert_eq!(self.version, storage.version);
        debug_assert_ne!(self.position, ITER_POSITION_INVALID);
        debug_assert!(self.position < storage.bucket_count);

        let node_hash = storage.hashes[self.position as usize];
        let key = storage.keys[self.position as usize];
        insert_node(
            inner,
            &mut storage,
            InsertParams {
                node_index: self.position,
                key_hash: node_hash,
                new_key: key,
                new_value: value,
                keep_new_key: true,
                reusing_key: true,
            },
        );
        self.version = storage.version;
    }

    /// Remove the current entry without destroy notifies (`g_hash_table_iter_steal`).
    pub fn steal(&mut self) {
        self.remove_or_steal(false);
    }

    fn remove_or_steal(&mut self, notify: bool) {
        let inner = self.table.inner();
        let mut storage = inner.storage.borrow_mut();
        debug_assert_eq!(self.version, storage.version);
        debug_assert_ne!(self.position, ITER_POSITION_INVALID);
        debug_assert!(self.position < storage.bucket_count);

        remove_node(inner, &mut storage, self.position, notify);
        bump_version(&mut storage);
        self.version = storage.version;
    }
}

fn insert_node(
    inner: &HashTableInner,
    storage: &mut HashTableStorage,
    params: InsertParams,
) -> bool {
    let InsertParams {
        node_index,
        key_hash,
        new_key,
        new_value,
        keep_new_key,
        reusing_key,
    } = params;
    let old_hash = storage.hashes[node_index as usize];
    let already_exists = hash_is_real(old_hash);

    let mut key_to_free = core::ptr::null_mut();
    let key_to_keep;
    let mut value_to_free = core::ptr::null_mut();

    if already_exists {
        value_to_free = fetch_value(storage, node_index);
        if keep_new_key {
            key_to_free = storage.keys[node_index as usize];
            key_to_keep = new_key;
        } else {
            key_to_free = new_key;
            key_to_keep = storage.keys[node_index as usize];
        }
    } else {
        storage.hashes[node_index as usize] = key_hash;
        key_to_keep = new_key;
    }

    ensure_keyval_fits(storage, key_to_keep, new_value);
    storage.keys[node_index as usize] = key_to_keep;
    if storage.is_set {
        // value stored in keys slot
    } else {
        storage.values[node_index as usize] = new_value;
    }

    if !already_exists {
        storage.nnodes += 1;
        if old_hash == UNUSED_HASH_VALUE {
            storage.noccupied += 1;
            maybe_resize(storage);
        }
        bump_version(storage);
    }

    if already_exists {
        if inner.key_destroy_func.borrow_mut().is_some() && !reusing_key {
            call_key_destroy(inner, key_to_free);
        }
        call_value_destroy(inner, value_to_free);
    }

    !already_exists
}

fn remove_all_nodes(
    inner: &HashTableInner,
    storage: &mut HashTableStorage,
    notify: bool,
    destruction: bool,
) {
    if storage.nnodes == 0 {
        return;
    }

    storage.nnodes = 0;
    storage.noccupied = 0;

    let key_destroy = inner.key_destroy_func.borrow().is_some();
    let value_destroy = inner.value_destroy_func.borrow().is_some();

    if !notify || (!key_destroy && !value_destroy) {
        if !destruction {
            for slot in &mut storage.hashes {
                *slot = UNUSED_HASH_VALUE;
            }
            for slot in &mut storage.keys {
                *slot = core::ptr::null_mut();
            }
            if !storage.is_set {
                for slot in &mut storage.values {
                    *slot = core::ptr::null_mut();
                }
            }
        }
        return;
    }

    let old_size = storage.bucket_count;
    let old_is_set = storage.is_set;
    let old_keys = core::mem::replace(
        &mut storage.keys,
        vec![core::ptr::null_mut(); storage.bucket_count as usize],
    );
    let old_values = if old_is_set {
        Vec::new()
    } else {
        core::mem::replace(
            &mut storage.values,
            vec![core::ptr::null_mut(); storage.bucket_count as usize],
        )
    };
    let old_hashes = core::mem::replace(
        &mut storage.hashes,
        vec![UNUSED_HASH_VALUE; storage.bucket_count as usize],
    );

    if !destruction {
        setup_storage(storage);
    } else {
        storage.bucket_count = 0;
        storage.mod_ = 0;
        storage.mask = 0;
    }

    for i in 0..old_size {
        if hash_is_real(old_hashes[i as usize]) {
            let key = old_keys[i as usize];
            let value = if old_is_set {
                key
            } else {
                old_values[i as usize]
            };
            call_key_destroy(inner, key);
            call_value_destroy(inner, value);
        }
    }
}

unsafe fn release_hash_table(inner: NonNull<HashTableInner>) {
    let table = HashTable { inner };
    {
        let inner_ref = table.inner();
        let mut storage = inner_ref.storage.borrow_mut();
        remove_all_nodes(inner_ref, &mut storage, true, true);
    }
    unsafe {
        drop(Box::from_raw(table.inner.as_ptr()));
    }
}

// ---------------------------------------------------------------------------
// Built-in hash / equal helpers
// ---------------------------------------------------------------------------

/// C string equality (`g_str_equal`).
pub fn str_equal(v1: *const (), v2: *const ()) -> bool {
    if v1.is_null() || v2.is_null() {
        return v1 == v2;
    }
    unsafe {
        let s1 = core::ffi::CStr::from_ptr(v1.cast());
        let s2 = core::ffi::CStr::from_ptr(v2.cast());
        s1 == s2
    }
}

/// C string djb2 hash (`g_str_hash`), matching [`crate::bytes::Bytes::hash`].
pub fn str_hash(v: *const ()) -> u32 {
    if v.is_null() {
        return 0;
    }
    unsafe {
        let mut h: u32 = 5381;
        let mut p = v.cast::<i8>();
        while *p != 0 {
            h = h
                .wrapping_shl(5)
                .wrapping_add(h)
                .wrapping_add(i32::from(*p) as u32);
            p = p.add(1);
        }
        h
    }
}

/// Pointer hash (`g_direct_hash`).
pub fn direct_hash(v: *const ()) -> u32 {
    v as usize as u32
}

/// Pointer equality (`g_direct_equal`).
pub fn direct_equal(v1: *const (), v2: *const ()) -> bool {
    v1 == v2
}

/// `gint` equality via pointer (`g_int_equal`).
pub fn int_equal(v1: *const (), v2: *const ()) -> bool {
    unsafe { *(v1.cast::<i32>()) == *(v2.cast::<i32>()) }
}

/// `gint` hash via pointer (`g_int_hash`).
pub fn int_hash(v: *const ()) -> u32 {
    unsafe { *(v.cast::<i32>()) as u32 }
}

/// `gint64` equality via pointer (`g_int64_equal`).
pub fn int64_equal(v1: *const (), v2: *const ()) -> bool {
    unsafe { *(v1.cast::<i64>()) == *(v2.cast::<i64>()) }
}

/// `gint64` hash via pointer (`g_int64_hash`).
pub fn int64_hash(v: *const ()) -> u32 {
    hash_u64_bits(unsafe { *(v.cast::<i64>()) as u64 })
}

/// `gdouble` equality via pointer (`g_double_equal`).
pub fn double_equal(v1: *const (), v2: *const ()) -> bool {
    unsafe { *(v1.cast::<f64>()) == *(v2.cast::<f64>()) }
}

/// `gdouble` hash via pointer (`g_double_hash`).
pub fn double_hash(v: *const ()) -> u32 {
    hash_u64_bits(unsafe { (*v.cast::<f64>()).to_bits() })
}

fn hash_u64_bits(bits: u64) -> u32 {
    ((bits >> 32) ^ (bits & 0xffff_ffff)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn int_to_ptr(n: i32) -> GPointer {
        n as isize as GPointer
    }

    fn ptr_to_int(p: GPointer) -> i32 {
        p as isize as i32
    }

    fn cstr(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn direct_hash_lookup() {
        let h = HashTable::new(None, None);
        for i in 1..=20 {
            assert!(h.insert(int_to_ptr(i), int_to_ptr(i + 42)));
        }
        assert_eq!(h.size(), 20);
        for i in 1..=20 {
            let rc = ptr_to_int(h.lookup(int_to_ptr(i).cast_const()));
            assert_eq!(rc - 42, i);
        }
        h.destroy();
    }

    #[test]
    fn direct_hash_with_funcs() {
        let h = HashTable::new(Some(direct_hash), Some(direct_equal));
        for i in 1..=20 {
            h.insert(int_to_ptr(i), int_to_ptr(i + 42));
        }
        assert_eq!(h.size(), 20);
        for i in 1..=20 {
            assert_eq!(ptr_to_int(h.lookup(int_to_ptr(i).cast_const())) - 42, i);
        }
        h.destroy();
    }

    #[test]
    fn int_hash_lookup() {
        let h = HashTable::new(Some(int_hash), Some(int_equal));
        let mut values = [0i32; 20];
        for (i, slot) in values.iter_mut().enumerate() {
            *slot = i as i32 + 42;
            h.insert(slot as *mut i32 as GPointer, int_to_ptr(i as i32 + 42));
        }
        assert_eq!(h.size(), 20);
        for i in 0..20 {
            let key = i as i32 + 42;
            assert_eq!(
                ptr_to_int(h.lookup((&key as *const i32).cast())),
                i as i32 + 42
            );
        }
        h.destroy();
    }

    #[test]
    fn int64_hash_no_high_word_collision() {
        let m: i64 = 722;
        let n: i64 = (2003i64 << 32) + 722;
        assert_ne!(
            int64_hash((&m as *const i64).cast()),
            int64_hash((&n as *const i64).cast())
        );
    }

    #[test]
    fn str_hash_insert_lookup_remove() {
        let h = HashTable::new_full(Some(str_hash), Some(str_equal), None, None);
        for i in 0..20 {
            let key = cstr(&i.to_string());
            let val = cstr(&format!("{i} value"));
            h.insert(key.into_raw() as GPointer, val.into_raw() as GPointer);
        }
        assert_eq!(h.size(), 20);

        for i in 0..20 {
            let key = cstr(&i.to_string());
            let v = h.lookup(key.as_ptr().cast());
            assert!(!v.is_null());
            let val = unsafe { CStr::from_ptr(v.cast()) };
            assert_eq!(val.to_str().unwrap(), format!("{i} value"));
        }

        let remove_key = cstr("3");
        assert!(h.remove(remove_key.as_ptr().cast()));
        assert_eq!(h.size(), 19);

        h.foreach_remove(|key, _| {
            let s = unsafe { CStr::from_ptr(key.cast()) };
            let n: i32 = s.to_str().unwrap().parse().unwrap();
            n % 2 == 0
        });
        assert_eq!(h.size(), 9);
        h.destroy();
    }

    #[test]
    fn set_add_and_contains() {
        let counter = Arc::new(AtomicU32::new(0));
        let h = HashTable::new_full(
            Some(str_hash),
            Some(str_equal),
            Some(Box::new({
                let counter = Arc::clone(&counter);
                move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            })),
            None,
        );

        for i in (2..5000).step_by(7) {
            let s = cstr(&i.to_string());
            assert!(h.add(s.into_raw() as GPointer));
        }
        assert!(!h.add(cstr("2").into_raw() as GPointer));
        assert_eq!(h.size(), (2..5000).step_by(7).count() as u32);

        let key = cstr("2");
        assert!(h.contains(key.as_ptr().cast()));
        assert!(!h.contains(cstr("a").as_ptr().cast()));

        h.insert(
            cstr("a").into_raw() as GPointer,
            cstr("b").into_raw() as GPointer,
        );
        assert_eq!(
            unsafe { CStr::from_ptr(h.lookup(cstr("a").as_ptr().cast()).cast()) },
            cstr("b").as_c_str()
        );
        h.destroy();
    }

    #[test]
    fn find_and_foreach() {
        let h = HashTable::new(Some(str_hash), Some(str_equal));
        for ch in ['a', 'b', 'c', 'd', 'e', 'f'] {
            let k = cstr(&ch.to_string());
            let v = cstr(&ch.to_string().to_uppercase());
            h.insert(k.into_raw() as GPointer, v.into_raw() as GPointer);
        }

        let found = h.find(|key, _| {
            let s = unsafe { CStr::from_ptr(key.cast()) };
            s.to_bytes() == b"c"
        });
        assert_eq!(
            unsafe { CStr::from_ptr(found.cast()) },
            cstr("C").as_c_str()
        );

        let mut seen = [false; 6];
        h.foreach(|key, _| {
            let s = unsafe { CStr::from_ptr(key.cast()) };
            let idx = (s.to_bytes()[0] - b'a') as usize;
            seen[idx] = true;
        });
        assert!(seen.iter().all(|&v| v));
        h.destroy();
    }

    #[test]
    fn lookup_extended_and_steal_extended() {
        let h = HashTable::new_full(
            Some(str_hash),
            Some(str_equal),
            Some(Box::new(|p| unsafe {
                drop(CString::from_raw(p.cast()));
            })),
            Some(Box::new(|p| unsafe {
                drop(CString::from_raw(p.cast()));
            })),
        );

        for ch in ['a', 'b', 'c'] {
            let k = cstr(&ch.to_string());
            let v = cstr(&ch.to_string().to_uppercase());
            h.insert(k.into_raw() as GPointer, v.into_raw() as GPointer);
        }

        let mut orig_key = core::ptr::null_mut();
        let mut value = core::ptr::null_mut();
        let lookup = cstr("b");
        assert!(h.lookup_extended(
            lookup.as_ptr().cast(),
            Some(&mut orig_key),
            Some(&mut value),
        ));
        assert_eq!(
            unsafe { CStr::from_ptr(orig_key.cast()) },
            cstr("b").as_c_str()
        );
        assert_eq!(
            unsafe { CStr::from_ptr(value.cast()) },
            cstr("B").as_c_str()
        );

        let mut stolen_key = core::ptr::null_mut();
        let mut stolen_value = core::ptr::null_mut();
        let steal_lookup = cstr("a");
        assert!(h.steal_extended(
            steal_lookup.as_ptr().cast(),
            Some(&mut stolen_key),
            Some(&mut stolen_value),
        ));
        unsafe {
            drop(CString::from_raw(stolen_key.cast()));
            drop(CString::from_raw(stolen_value.cast()));
        }
        assert_eq!(h.size(), 2);
        h.destroy();
    }

    #[test]
    fn iter_remove_and_replace() {
        let h = HashTable::new(Some(int_hash), Some(int_equal));
        let mut globals = [0i32; 100];
        for (i, slot) in globals.iter_mut().enumerate() {
            *slot = i as i32;
            h.insert(slot as *mut i32 as GPointer, slot as *mut i32 as GPointer);
        }

        let mut iter = h.iter();
        let mut key = core::ptr::null_mut();
        let mut value = core::ptr::null_mut();
        let mut kept = 0u32;
        while iter.next(Some(&mut key), Some(&mut value)) {
            let n = unsafe { *(key.cast::<i32>()) };
            if n % 2 != 0 {
                iter.remove();
            } else {
                kept += 1;
            }
        }
        assert_eq!(h.size(), kept);

        let replacement = 99i32;
        let mut iter = h.iter();
        while iter.next(Some(&mut key), Some(&mut value)) {
            iter.replace(&replacement as *const i32 as GPointer);
        }
        h.foreach(|_, v| {
            assert_eq!(unsafe { *(v.cast::<i32>()) }, 99);
        });
        h.destroy();
    }

    #[test]
    fn foreach_steal_moves_entries() {
        let h = HashTable::new_full(
            Some(str_hash),
            Some(str_equal),
            Some(Box::new(|p| unsafe {
                drop(CString::from_raw(p.cast()));
            })),
            Some(Box::new(|p| unsafe {
                drop(CString::from_raw(p.cast()));
            })),
        );
        let h2 = HashTable::new_full(Some(str_hash), Some(str_equal), None, None);

        for ch in ['a', 'b', 'c', 'd', 'e', 'f'] {
            let k = cstr(&ch.to_string());
            let v = cstr(&ch.to_string().to_uppercase());
            h.insert(k.into_raw() as GPointer, v.into_raw() as GPointer);
        }

        h.foreach_steal(|key, _value| {
            let s = unsafe { CStr::from_ptr(key.cast()) };
            matches!(s.to_bytes()[0], b'a' | b'c' | b'e')
        });
        // Stolen entries remain in memory; h2 receives manual re-insert in C test.
        // Here we only verify removal count.
        assert_eq!(h.size(), 3);
        drop(h2);
        h.destroy();
    }

    #[test]
    fn ref_unref_and_destroy_notifies() {
        let destroyed = Arc::new(AtomicU32::new(0));
        let destroyed2 = Arc::clone(&destroyed);
        let h = HashTable::new_full(
            Some(str_hash),
            Some(str_equal),
            None,
            Some(Box::new(move |_| {
                destroyed2.fetch_add(1, Ordering::SeqCst);
            })),
        );

        let a = cstr("abc");
        let b = cstr("cde");
        let c = cstr("xyz");
        h.insert(a.as_ptr() as GPointer, b"ABC".as_ptr() as GPointer);
        h.insert(b.as_ptr() as GPointer, b"CDE".as_ptr() as GPointer);
        h.insert(c.as_ptr() as GPointer, b"XYZ".as_ptr() as GPointer);

        let mut iter = h.iter();
        let mut key = core::ptr::null_mut();
        let mut value = core::ptr::null_mut();
        while iter.next(Some(&mut key), Some(&mut value)) {
            let s = unsafe { CStr::from_ptr(key.cast()) };
            if s.to_bytes() == b"abc" {
                iter.steal();
            }
        }
        assert_eq!(destroyed.load(Ordering::SeqCst), 0);

        let extra = h.ref_();
        h.destroy();
        assert_eq!(extra.size(), 0);
        assert_eq!(destroyed.load(Ordering::SeqCst), 2);

        extra.insert(
            cstr("uvw").as_ptr() as GPointer,
            b"UVW".as_ptr() as GPointer,
        );
        extra.unref();
        assert_eq!(destroyed.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn remove_all_steal_and_remove() {
        let key_destroyed = Arc::new(AtomicU32::new(0));
        let value_destroyed = Arc::new(AtomicU32::new(0));
        let kd = Arc::clone(&key_destroyed);
        let vd = Arc::clone(&value_destroyed);

        let h = HashTable::new_full(
            Some(str_hash),
            Some(str_equal),
            Some(Box::new(move |_| {
                kd.fetch_add(1, Ordering::SeqCst);
            })),
            Some(Box::new(move |_| {
                vd.fetch_add(1, Ordering::SeqCst);
            })),
        );

        h.insert(
            cstr("abc").as_ptr() as GPointer,
            cstr("ABC").as_ptr() as GPointer,
        );
        h.insert(
            cstr("cde").as_ptr() as GPointer,
            cstr("CDE").as_ptr() as GPointer,
        );
        h.steal_all();
        assert_eq!(key_destroyed.load(Ordering::SeqCst), 0);
        assert_eq!(value_destroyed.load(Ordering::SeqCst), 0);

        h.insert(
            cstr("xyz").as_ptr() as GPointer,
            cstr("XYZ").as_ptr() as GPointer,
        );
        assert!(!h.steal(cstr("nosuch").as_ptr().cast()));
        assert!(h.steal(cstr("xyz").as_ptr().cast()));

        h.insert(
            cstr("a").as_ptr() as GPointer,
            cstr("A").as_ptr() as GPointer,
        );
        h.insert(
            cstr("b").as_ptr() as GPointer,
            cstr("B").as_ptr() as GPointer,
        );
        h.remove_all();
        assert_eq!(key_destroyed.load(Ordering::SeqCst), 2);
        assert_eq!(value_destroyed.load(Ordering::SeqCst), 2);
        h.destroy();
    }

    #[test]
    fn str_hash_matches_bytes_djb2() {
        let s = cstr("hello");
        assert_eq!(
            str_hash(s.as_ptr().cast()),
            crate::bytes::Bytes::new(b"hello").hash()
        );
    }

    #[test]
    fn large_insert_remove_stress() {
        let h = HashTable::new(Some(int_hash), Some(int_equal));
        let mut globals = vec![0i32; 10_000];
        for (i, slot) in globals.iter_mut().enumerate() {
            *slot = i as i32;
            h.insert(slot as *mut i32 as GPointer, slot as *mut i32 as GPointer);
        }
        assert_eq!(h.size(), 10_000);

        let target = 120i32;
        let found = h.find(|_, value| unsafe { *(value.cast::<i32>()) == target });
        assert_eq!(unsafe { *(found.cast::<i32>()) }, target);

        h.foreach_remove(|_, value| unsafe { *(value.cast::<i32>()) % 2 != 0 });
        assert_eq!(h.size(), 5000);
        h.destroy();
    }
}
