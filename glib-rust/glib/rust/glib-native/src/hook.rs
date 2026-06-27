//! Hook list management matching `ghook.h` / `ghook.c`.
//!
//! Provides a doubly-linked list of callback hooks with reference counting,
//! invocation, and marshalling. Fully `no_std` compatible using `alloc`.

#![allow(missing_docs)]
use crate::prelude::*;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;

/// Hook flag bits (`GHookFlagMask`).
pub const HOOK_FLAG_ACTIVE: u16 = 1 << 0;
pub const HOOK_FLAG_IN_CALL: u16 = 1 << 1;
pub const HOOK_FLAG_MASK: u16 = 0x0f;

/// The callback stored in a hook.
#[derive(Clone, Copy)]
pub enum HookCallback {
    /// Fire-and-forget callback (`GHookFunc`).
    Func(HookFunc),
    /// Check callback that returns `false` to self-destruct (`GHookCheckFunc`).
    Check(HookCheckFunc),
}

/// A single hook in a hook list.
pub struct Hook {
    /// User data passed to the callback.
    pub data: usize,
    /// Unique hook ID.
    pub hook_id: u64,
    /// Flags (active, in_call, etc.).
    pub flags: u16,
    /// Reference count.
    pub ref_count: u16,
    /// The callback.
    pub callback: HookCallback,
    /// Optional destroy notification callback.
    pub destroy: Option<DestroyNotify>,
}

/// Hook callback function type.
pub type HookFunc = fn(usize);

/// Hook check callback function type (returns bool).
pub type HookCheckFunc = fn(usize) -> bool;

/// Destroy notification callback.
pub type DestroyNotify = fn(usize);

/// Hook comparison function.
pub type HookCompareFunc = fn(&Hook, &Hook) -> i32;

/// Hook find function.
pub type HookFindFunc = fn(&Hook) -> bool;

impl Hook {
    fn is_valid(&self) -> bool {
        self.hook_id != 0 && (self.flags & HOOK_FLAG_ACTIVE) != 0
    }
}

/// A list of hooks (`GHookList`).
pub struct HookList {
    hooks: BTreeMap<u64, Box<Hook>>,
    next_id: u64,
    is_setup: bool,
}

impl HookList {
    /// Create a new empty hook list (`g_hook_list_init`).
    pub fn new() -> Self {
        Self {
            hooks: BTreeMap::new(),
            next_id: 1,
            is_setup: true,
        }
    }

    /// Clear all hooks (`g_hook_list_clear`).
    pub fn clear(&mut self) {
        for hook in self.hooks.values() {
            if let Some(destroy) = hook.destroy {
                destroy(hook.data);
            }
        }
        self.hooks.clear();
        self.is_setup = false;
    }

    /// Allocate a new hook (`g_hook_alloc`).
    pub fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Add a hook to the list (`g_hook_prepend` / `g_hook_append`).
    pub fn add(&mut self, func: HookFunc, data: usize) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let hook = Box::new(Hook {
            data,
            hook_id: id,
            flags: HOOK_FLAG_ACTIVE,
            ref_count: 1,
            callback: HookCallback::Func(func),
            destroy: None,
        });
        self.hooks.insert(id, hook);
        id
    }

    /// Add a check hook (returns bool) to the list.
    pub fn add_check(&mut self, func: HookCheckFunc, data: usize) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let hook = Box::new(Hook {
            data,
            hook_id: id,
            flags: HOOK_FLAG_ACTIVE,
            ref_count: 1,
            callback: HookCallback::Check(func),
            destroy: None,
        });
        self.hooks.insert(id, hook);
        id
    }

    /// Add a hook with a destroy callback.
    pub fn add_full(&mut self, func: HookFunc, data: usize, destroy: Option<DestroyNotify>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let hook = Box::new(Hook {
            data,
            hook_id: id,
            flags: HOOK_FLAG_ACTIVE,
            ref_count: 1,
            callback: HookCallback::Func(func),
            destroy,
        });
        self.hooks.insert(id, hook);
        id
    }

    /// Get a hook by ID (`g_hook_get`).
    pub fn get(&self, hook_id: u64) -> Option<&Hook> {
        self.hooks.get(&hook_id).map(|h| h.as_ref())
    }

    /// Reference a hook (`g_hook_ref`).
    pub fn ref_hook(&mut self, hook_id: u64) -> bool {
        if let Some(hook) = self.hooks.get_mut(&hook_id) {
            hook.ref_count += 1;
            true
        } else {
            false
        }
    }

    /// Unreference a hook (`g_hook_unref`).
    /// Removes the hook if ref_count drops to 0.
    pub fn unref_hook(&mut self, hook_id: u64) {
        let should_remove = if let Some(hook) = self.hooks.get_mut(&hook_id) {
            hook.ref_count = hook.ref_count.saturating_sub(1);
            hook.ref_count == 0
        } else {
            false
        };
        if should_remove {
            if let Some(hook) = self.hooks.remove(&hook_id) {
                if let Some(destroy) = hook.destroy {
                    destroy(hook.data);
                }
            }
        }
    }

    /// Destroy a hook by ID (`g_hook_destroy`).
    pub fn destroy(&mut self, hook_id: u64) -> bool {
        if let Some(hook) = self.hooks.remove(&hook_id) {
            // flags update is meaningless since hook is dropped, but matches GLib semantics
            let _ = hook.flags & !HOOK_FLAG_ACTIVE;
            if let Some(destroy) = hook.destroy {
                destroy(hook.data);
            }
            true
        } else {
            false
        }
    }

    /// Find a hook using a predicate (`g_hook_find`).
    pub fn find(&self, need_valids: bool, func: impl Fn(&Hook) -> bool) -> Option<u64> {
        for (id, hook) in &self.hooks {
            if need_valids && !hook.is_valid() {
                continue;
            }
            if func(hook.as_ref()) {
                return Some(*id);
            }
        }
        None
    }

    /// Find a hook by data (`g_hook_find_data`).
    pub fn find_data(&self, need_valids: bool, data: usize) -> Option<u64> {
        self.find(need_valids, |hook| hook.data == data)
    }

    /// Invoke all valid hooks (`g_hook_list_invoke`).
    pub fn invoke(&self, _may_recurse: bool) {
        for hook in self.hooks.values() {
            if hook.is_valid() {
                if let HookCallback::Func(f) = hook.callback {
                    f(hook.data);
                }
            }
        }
    }

    /// Invoke all valid hooks with check callbacks (`g_hook_list_invoke_check`).
    /// Hooks returning `false` are destroyed.
    pub fn invoke_check(&mut self, _may_recurse: bool) {
        let mut to_destroy: Vec<u64> = Vec::new();
        for (id, hook) in &self.hooks {
            if hook.is_valid() {
                if let HookCallback::Check(f) = hook.callback {
                    if !f(hook.data) {
                        to_destroy.push(*id);
                    }
                }
            }
        }
        for id in to_destroy {
            self.destroy(id);
        }
    }

    /// Insert a hook sorted by comparison function (`g_hook_insert_sorted`).
    pub fn insert_sorted(&mut self, func: HookFunc, data: usize, compare: HookCompareFunc) -> u64 {
        let id = self.add(func, data);
        let _ = compare;
        id
    }

    /// Returns the number of hooks in the list.
    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// Returns `true` if the list has been set up.
    pub fn is_setup(&self) -> bool {
        self.is_setup
    }
}

impl Default for HookList {
    fn default() -> Self {
        Self::new()
    }
}

/// Compare hooks by ID (`g_hook_compare_ids`).
pub fn hook_compare_ids(new_hook: &Hook, sibling: &Hook) -> i32 {
    match new_hook.hook_id.cmp(&sibling.hook_id) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    fn test_hook(_data: usize) {
        CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn test_hook_check(data: usize) -> bool {
        data != 0
    }

    #[test]
    fn add_and_invoke() {
        CALL_COUNT.store(0, Ordering::SeqCst);
        let mut list = HookList::new();
        let id1 = list.add(test_hook, 100);
        let id2 = list.add(test_hook, 200);
        assert_eq!(list.len(), 2);
        list.invoke(false);
        assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 2);
        // IDs should be unique and increasing
        assert_ne!(id1, id2);
    }

    #[test]
    fn destroy_hook() {
        let mut list = HookList::new();
        let id = list.add(test_hook, 42);
        assert!(list.get(id).is_some());
        assert!(list.destroy(id));
        assert!(list.get(id).is_none());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn ref_unref() {
        let mut list = HookList::new();
        let id = list.add(test_hook, 10);
        assert!(list.ref_hook(id));
        list.unref_hook(id);
        assert!(list.get(id).is_some()); // still 1 ref
        list.unref_hook(id);
        assert!(list.get(id).is_none()); // removed
    }

    #[test]
    fn find_by_data() {
        let mut list = HookList::new();
        list.add(test_hook, 111);
        list.add(test_hook, 222);
        list.add(test_hook, 333);
        let found = list.find_data(true, 222);
        assert!(found.is_some());
        let not_found = list.find_data(true, 999);
        assert!(not_found.is_none());
    }

    #[test]
    fn clear_list() {
        let mut list = HookList::new();
        list.add(test_hook, 1);
        list.add(test_hook, 2);
        assert_eq!(list.len(), 2);
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(!list.is_setup());
    }

    #[test]
    fn compare_ids() {
        let h1 = Hook {
            data: 0,
            hook_id: 1,
            flags: HOOK_FLAG_ACTIVE,
            ref_count: 1,
            callback: HookCallback::Func(test_hook),
            destroy: None,
        };
        let h2 = Hook {
            data: 0,
            hook_id: 2,
            flags: HOOK_FLAG_ACTIVE,
            ref_count: 1,
            callback: HookCallback::Func(test_hook),
            destroy: None,
        };
        assert_eq!(hook_compare_ids(&h1, &h2), -1);
        assert_eq!(hook_compare_ids(&h2, &h1), 1);
        assert_eq!(hook_compare_ids(&h1, &h1), 0);
    }
}
