//! GSignal — signal emission and connection.
//!
//! Signals are named events that can be emitted on GObject instances.
//! Handlers (closures) can be connected to signals and will be called
//! when the signal is emitted.

use crate::gtype::*;
use crate::gvalue::GValue;
use crate::prelude::*;
use alloc::sync::Arc;
use spin::rwlock::RwLock;

/// Signal ID type.
pub type SignalID = u32;

/// Handler ID type.
pub type HandlerID = u32;

/// Signal flags (`GSignalFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SignalFlags(pub u32);

impl SignalFlags {
    pub const NONE: Self = Self(0);
    pub const RUN_FIRST: Self = Self(1 << 0);
    pub const RUN_LAST: Self = Self(1 << 1);
    pub const RUN_CLEANUP: Self = Self(1 << 2);
    pub const NO_RECURSE: Self = Self(1 << 3);
    pub const DETAILED: Self = Self(1 << 4);
    pub const ACTION: Self = Self(1 << 5);
    pub const NO_HOOKS: Self = Self(1 << 6);
    pub const MUST_COLLECT: Self = Self(1 << 7);
    pub const DEPRECATED: Self = Self(1 << 8);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for SignalFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

/// Connection flags (`GConnectFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ConnectFlags(pub u32);

impl ConnectFlags {
    pub const NONE: Self = Self(0);
    pub const SWAPPED: Self = Self(1 << 0);
    pub const AFTER: Self = Self(1 << 1);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for ConnectFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

/// A signal handler callback.
pub type SignalCallback = Arc<dyn Fn(&[GValue]) -> Option<GValue> + Send + Sync>;

/// Signal query info (`GSignalQuery`).
#[derive(Clone)]
pub struct SignalQuery {
    pub signal_id: SignalID,
    pub signal_name: String,
    pub owner_type: GType,
    pub return_type: GType,
    pub param_types: Vec<GType>,
    pub flags: SignalFlags,
}

/// A registered signal.
#[derive(Clone)]
struct SignalEntry {
    id: SignalID,
    name: String,
    owner_type: GType,
    return_type: GType,
    param_types: Vec<GType>,
    flags: SignalFlags,
}

/// A connected handler.
struct HandlerEntry {
    id: HandlerID,
    signal_id: SignalID,
    instance_type: GType,
    callback: SignalCallback,
    after: bool,
}

/// Global signal registry.
static SIGNAL_REGISTRY: RwLock<Option<SignalRegistry>> = RwLock::new(None);

struct SignalRegistry {
    signals: Vec<SignalEntry>,
    handlers: Vec<HandlerEntry>,
    next_signal_id: SignalID,
    next_handler_id: HandlerID,
}

fn ensure_registry() {
    let mut guard = SIGNAL_REGISTRY.write();
    if guard.is_none() {
        *guard = Some(SignalRegistry {
            signals: Vec::new(),
            handlers: Vec::new(),
            next_signal_id: 1,
            next_handler_id: 1,
        });
    }
}

/// Register a new signal (`g_signal_new`).
pub fn signal_new(
    signal_name: &str,
    owner_type: GType,
    flags: SignalFlags,
    return_type: GType,
    param_types: &[GType],
) -> SignalID {
    ensure_registry();
    let mut guard = SIGNAL_REGISTRY.write();
    let reg = guard.as_mut().unwrap();

    let id = reg.next_signal_id;
    reg.next_signal_id += 1;

    reg.signals.push(SignalEntry {
        id,
        name: signal_name.to_owned(),
        owner_type,
        return_type,
        param_types: param_types.to_vec(),
        flags,
    });

    id
}

/// Look up a signal ID by name and type (`g_signal_lookup`).
pub fn signal_lookup(name: &str, owner_type: GType) -> SignalID {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    reg.signals.iter()
        .find(|s| s.name == name && type_is_a(owner_type, s.owner_type))
        .map(|s| s.id)
        .unwrap_or(0)
}

/// Query a signal (`g_signal_query`).
pub fn signal_query(signal_id: SignalID) -> Option<SignalQuery> {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    reg.signals.iter()
        .find(|s| s.id == signal_id)
        .map(|s| SignalQuery {
            signal_id: s.id,
            signal_name: s.name.clone(),
            owner_type: s.owner_type,
            return_type: s.return_type,
            param_types: s.param_types.clone(),
            flags: s.flags,
        })
}

/// Get the name of a signal (`g_signal_name`).
pub fn signal_name(signal_id: SignalID) -> Option<String> {
    signal_query(signal_id).map(|q| q.signal_name)
}

/// Connect a handler to a signal (`g_signal_connect`).
pub fn signal_connect(
    instance_type: GType,
    signal_id: SignalID,
    callback: SignalCallback,
    flags: ConnectFlags,
) -> HandlerID {
    ensure_registry();
    let mut guard = SIGNAL_REGISTRY.write();
    let reg = guard.as_mut().unwrap();

    let id = reg.next_handler_id;
    reg.next_handler_id += 1;

    reg.handlers.push(HandlerEntry {
        id,
        signal_id,
        instance_type,
        callback,
        after: flags.contains(ConnectFlags::AFTER),
    });

    id
}

/// Connect a handler by signal name.
pub fn signal_connect_by_name(
    instance_type: GType,
    signal_name: &str,
    callback: SignalCallback,
    flags: ConnectFlags,
) -> HandlerID {
    let signal_id = signal_lookup(signal_name, instance_type);
    if signal_id == 0 {
        return 0;
    }
    signal_connect(instance_type, signal_id, callback, flags)
}

/// Disconnect a handler (`g_signal_handler_disconnect`).
pub fn signal_handler_disconnect(handler_id: HandlerID) -> bool {
    ensure_registry();
    let mut guard = SIGNAL_REGISTRY.write();
    let reg = guard.as_mut().unwrap();
    let len_before = reg.handlers.len();
    reg.handlers.retain(|h| h.id != handler_id);
    reg.handlers.len() != len_before
}

/// Check if a handler is connected (`g_signal_handler_is_connected`).
pub fn signal_handler_is_connected(handler_id: HandlerID) -> bool {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    reg.handlers.iter().any(|h| h.id == handler_id)
}

/// Block a handler (prevent it from being called) (`g_signal_handler_block`).
pub fn signal_handler_block(_handler_id: HandlerID) {
    // In a full implementation, this would mark the handler as blocked.
    // For now, we don't implement blocking.
}

/// Unblock a handler (`g_signal_handler_unblock`).
pub fn signal_handler_unblock(_handler_id: HandlerID) {
    // In a full implementation, this would unmark the handler as blocked.
}

/// Emit a signal (`g_signal_emit`).
///
/// Returns the return value from the last handler (for `RUN_LAST` signals)
/// or the first handler (for `RUN_FIRST` signals).
pub fn signal_emit(
    instance_type: GType,
    signal_id: SignalID,
    args: &[GValue],
) -> Option<GValue> {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();

    let signal = reg.signals.iter().find(|s| s.id == signal_id)?;
    let mut result: Option<GValue> = None;

    // RUN_FIRST: call handlers before default
    if signal.flags.contains(SignalFlags::RUN_FIRST) {
        for h in &reg.handlers {
            if h.signal_id == signal_id && type_is_a(instance_type, h.instance_type) && !h.after {
                result = (h.callback)(args);
            }
        }
    }

    // RUN_LAST: call handlers after default (in registration order)
    if signal.flags.contains(SignalFlags::RUN_LAST) {
        for h in &reg.handlers {
            if h.signal_id == signal_id && type_is_a(instance_type, h.instance_type) && !h.after {
                result = (h.callback)(args);
            }
        }
        // "after" handlers
        for h in &reg.handlers {
            if h.signal_id == signal_id && type_is_a(instance_type, h.instance_type) && h.after {
                result = (h.callback)(args);
            }
        }
    }

    // If no RUN_FIRST or RUN_LAST, just call all handlers
    if !signal.flags.contains(SignalFlags::RUN_FIRST) && !signal.flags.contains(SignalFlags::RUN_LAST) {
        for h in &reg.handlers {
            if h.signal_id == signal_id && type_is_a(instance_type, h.instance_type) {
                result = (h.callback)(args);
            }
        }
    }

    result
}

/// Emit a signal by name.
pub fn signal_emit_by_name(
    instance_type: GType,
    signal_name: &str,
    args: &[GValue],
) -> Option<GValue> {
    let signal_id = signal_lookup(signal_name, instance_type);
    if signal_id == 0 {
        return None;
    }
    signal_emit(instance_type, signal_id, args)
}

/// List all signals for a type (`g_signal_list_ids`).
pub fn signal_list_ids(owner_type: GType) -> Vec<SignalID> {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    reg.signals.iter()
        .filter(|s| type_is_a(owner_type, s.owner_type))
        .map(|s| s.id)
        .collect()
}

/// Get the number of handlers connected to a signal.
pub fn signal_n_handlers(signal_id: SignalID) -> usize {
    ensure_registry();
    let guard = SIGNAL_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    reg.handlers.iter().filter(|h| h.signal_id == signal_id).count()
}

/// Disconnect all handlers for a signal on a type (`g_signal_handlers_disconnect_matched`).
pub fn signal_handlers_disconnect_all(instance_type: GType, signal_id: SignalID) -> usize {
    ensure_registry();
    let mut guard = SIGNAL_REGISTRY.write();
    let reg = guard.as_mut().unwrap();
    let len_before = reg.handlers.len();
    reg.handlers.retain(|h| !(h.signal_id == signal_id && type_is_a(instance_type, h.instance_type)));
    len_before - reg.handlers.len()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_new_int;
    use core::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn register_and_lookup_signal() {
        type_init();
        let id = signal_new("changed", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        assert!(id > 0);
        assert_eq!(signal_lookup("changed", G_TYPE_OBJECT), id);
        assert_eq!(signal_lookup("nonexistent", G_TYPE_OBJECT), 0);
    }

    #[test]
    fn signal_query_info() {
        type_init();
        let id = signal_new("notify", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[G_TYPE_STRING]);
        let q = signal_query(id).unwrap();
        assert_eq!(q.signal_name, "notify");
        assert_eq!(q.owner_type, G_TYPE_OBJECT);
        assert_eq!(q.return_type, G_TYPE_NONE);
        assert_eq!(q.param_types, vec![G_TYPE_STRING]);
    }

    #[test]
    fn connect_and_emit() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let id = signal_new("test-signal", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        signal_connect(G_TYPE_OBJECT, id, Arc::new(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            None
        }), ConnectFlags::NONE);

        assert_eq!(signal_n_handlers(id), 1);
        signal_emit(G_TYPE_OBJECT, id, &[]);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn disconnect_handler() {
        type_init();
        let id = signal_new("disconnect-test", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        let handler_id = signal_connect(G_TYPE_OBJECT, id, Arc::new(|_| None), ConnectFlags::NONE);
        assert!(signal_handler_is_connected(handler_id));
        assert!(signal_handler_disconnect(handler_id));
        assert!(!signal_handler_is_connected(handler_id));
    }

    #[test]
    fn emit_by_name() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let id = signal_new("by-name-test", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        signal_connect(G_TYPE_OBJECT, id, Arc::new(move |_| {
            counter_clone.fetch_add(10, Ordering::SeqCst);
            None
        }), ConnectFlags::NONE);

        signal_emit_by_name(G_TYPE_OBJECT, "by-name-test", &[]);
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn multiple_handlers() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));

        let id = signal_new("multi-test", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);

        for _ in 0..3 {
            let c = counter.clone();
            signal_connect(G_TYPE_OBJECT, id, Arc::new(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }), ConnectFlags::NONE);
        }

        assert_eq!(signal_n_handlers(id), 3);
        signal_emit(G_TYPE_OBJECT, id, &[]);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn signal_return_value() {
        type_init();
        let id = signal_new("return-test", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_INT, &[]);

        signal_connect(G_TYPE_OBJECT, id, Arc::new(|_| {
            Some(value_new_int(42))
        }), ConnectFlags::NONE);

        let result = signal_emit(G_TYPE_OBJECT, id, &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().get_int(), 42);
    }

    #[test]
    fn list_signal_ids() {
        type_init();
        signal_new("sig-a", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        signal_new("sig-b", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        let ids = signal_list_ids(G_TYPE_OBJECT);
        assert!(ids.len() >= 2);
    }

    #[test]
    fn disconnect_all_for_signal() {
        type_init();
        let id = signal_new("disconnect-all", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
        signal_connect(G_TYPE_OBJECT, id, Arc::new(|_| None), ConnectFlags::NONE);
        signal_connect(G_TYPE_OBJECT, id, Arc::new(|_| None), ConnectFlags::NONE);
        assert_eq!(signal_n_handlers(id), 2);
        let n = signal_handlers_disconnect_all(G_TYPE_OBJECT, id);
        assert_eq!(n, 2);
        assert_eq!(signal_n_handlers(id), 0);
    }
}
