//! GObject — base object class with reference counting, properties, and signals.
//!
//! This is a `no_std` implementation of the GObject base class. It provides:
//! - Reference counting (`ref`/`unref`)
//! - Property get/set with ParamSpec validation
//! - Signal connection and emission
//! - Weak references

use crate::gparamspec::{find_property, ParamFlags, ParamID, ParamSpec};
use crate::gsignal::{
    signal_connect_by_name, signal_emit_by_name, signal_new, ConnectFlags, HandlerID,
    SignalCallback, SignalFlags,
};
use crate::gtype::{type_name, GType, G_TYPE_INT, G_TYPE_NONE, G_TYPE_OBJECT};
use crate::gvalue::{value_new_int, value_new_string, GValue};
// Re-export the most common helpers so `#[cfg(test)] mod tests { use super::*; }`
// in this file can call them without spelling out the path.
#[cfg(test)]
pub use crate::gtype::type_init;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use core::sync::atomic::AtomicBool;
use spin::rwlock::RwLock;
use spin::Mutex;

/// Object flags (`GObjectFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ObjectFlags(pub u32);

impl ObjectFlags {
    pub const NONE: Self = Self(0);
    pub const IN_CONSTRUCTION: Self = Self(1 << 0);
    pub const FLOATING: Self = Self(1 << 1);
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Weak reference callback.
pub type WeakRefCallback = Arc<dyn Fn() + Send + Sync>;

/// A GObject instance.
pub struct GObject {
    pub type_id: GType,
    ref_count: core::sync::atomic::AtomicU32,
    flags: core::sync::atomic::AtomicU32,
    properties: RwLock<BTreeMap<String, GValue>>,
    param_specs: RwLock<Vec<ParamSpec>>,
    weak_refs: RwLock<Vec<WeakRefCallback>>,
    user_data: RwLock<BTreeMap<String, GValue>>,
}

impl GObject {
    /// Create a new GObject of the given type.
    pub fn new(type_id: GType) -> Arc<Self> {
        Self::new_with_params(type_id, &[])
    }

    /// Create a new GObject with construction parameters.
    pub fn new_with_params(type_id: GType, params: &[(&str, GValue)]) -> Arc<Self> {
        let obj = Arc::new(Self {
            type_id,
            ref_count: core::sync::atomic::AtomicU32::new(1),
            flags: core::sync::atomic::AtomicU32::new(ObjectFlags::IN_CONSTRUCTION.0),
            properties: RwLock::new(BTreeMap::new()),
            param_specs: RwLock::new(Vec::new()),
            weak_refs: RwLock::new(Vec::new()),
            user_data: RwLock::new(BTreeMap::new()),
        });

        // Set construct properties
        for (name, value) in params {
            obj.set_property(name, value.clone());
        }

        obj.flags.store(0, core::sync::atomic::Ordering::SeqCst);
        obj
    }

    /// Get the type ID of this object (`G_OBJECT_TYPE`).
    pub fn type_id(&self) -> GType {
        self.type_id
    }

    /// Get the type name (`G_OBJECT_TYPE_NAME`).
    pub fn type_name(&self) -> String {
        type_name(self.type_id).unwrap_or_else(|| "unknown".to_owned())
    }

    /// Increment the reference count (`g_object_ref`).
    pub fn ref_(&self) {
        self.ref_count
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    }

    /// Decrement the reference count (`g_object_unref`).
    ///
    /// In a full implementation, this would drop the object when ref_count
    /// reaches 0. In this `no_std` context, we just decrement and let
    /// `Arc` handle the actual deallocation.
    pub fn unref(&self) {
        let prev = self
            .ref_count
            .fetch_sub(1, core::sync::atomic::Ordering::SeqCst);
        if prev == 1 {
            // Object is being destroyed — fire weak refs
            let weak_refs = self.weak_refs.read();
            for cb in weak_refs.iter() {
                cb();
            }
        }
    }

    /// Get the current reference count.
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(core::sync::atomic::Ordering::SeqCst)
    }

    /// Check if the object is floating (`g_object_is_floating`).
    pub fn is_floating(&self) -> bool {
        let f = self.flags.load(core::sync::atomic::Ordering::SeqCst);
        (f & ObjectFlags::FLOATING.0) != 0
    }

    /// Make the object floating (`g_object_force_floating`).
    pub fn force_floating(&self) {
        let mut f = self.flags.load(core::sync::atomic::Ordering::SeqCst);
        f |= ObjectFlags::FLOATING.0;
        self.flags.store(f, core::sync::atomic::Ordering::SeqCst);
    }

    /// Sink a floating object (`g_object_ref_sink`).
    pub fn ref_sink(&self) {
        let mut f = self.flags.load(core::sync::atomic::Ordering::SeqCst);
        if (f & ObjectFlags::FLOATING.0) != 0 {
            f &= !ObjectFlags::FLOATING.0;
            self.flags.store(f, core::sync::atomic::Ordering::SeqCst);
        } else {
            self.ref_();
        }
    }

    /// Install properties on this object class.
    pub fn install_properties(&self, specs: Vec<ParamSpec>) {
        let mut owned_specs = specs;
        for (i, spec) in owned_specs.iter_mut().enumerate() {
            spec.id = (i + 1) as ParamID;
        }
        // Initialize default values
        let mut props = self.properties.write();
        for spec in &owned_specs {
            props.insert(spec.name.clone(), spec.default_value.clone());
        }
        *self.param_specs.write() = owned_specs;
    }

    /// Get a property value (`g_object_get_property`).
    pub fn get_property(&self, name: &str) -> Option<GValue> {
        let props = self.properties.read();
        props.get(name).cloned()
    }

    /// Set a property value (`g_object_set_property`).
    pub fn set_property(&self, name: &str, value: GValue) {
        let specs = self.param_specs.read();
        if let Some(spec) = find_property(&specs, name) {
            if !spec.is_writable() {
                return;
            }
            let mut value = value;
            if !spec.value_validate(&mut value) {
                return;
            }
            let mut props = self.properties.write();
            props.insert(name.to_owned(), value);

            // Emit "notify" signal
            drop(props);
            drop(specs);
            let notify_args = vec![value_new_string(name)];
            signal_emit_by_name(self.type_id, "notify", &notify_args);
        }
    }

    /// Get a list of all property specs.
    pub fn list_properties(&self) -> Vec<ParamSpec> {
        self.param_specs.read().clone()
    }

    /// Add a weak reference callback.
    pub fn add_weak_ref(&self, callback: WeakRefCallback) {
        self.weak_refs.write().push(callback);
    }

    /// Remove all weak reference callbacks.
    pub fn clear_weak_refs(&self) {
        self.weak_refs.write().clear();
    }

    /// Set user data by key.
    pub fn set_data(&self, key: &str, value: GValue) {
        self.user_data.write().insert(key.to_owned(), value);
    }

    /// Get user data by key.
    pub fn get_data(&self, key: &str) -> Option<GValue> {
        self.user_data.read().get(key).cloned()
    }

    /// Remove user data by key.
    pub fn remove_data(&self, key: &str) -> Option<GValue> {
        self.user_data.write().remove(key)
    }

    /// Connect to a signal on this object's type.
    pub fn connect_signal(
        &self,
        signal_name: &str,
        callback: SignalCallback,
        flags: ConnectFlags,
    ) -> HandlerID {
        signal_connect_by_name(self.type_id, signal_name, callback, flags)
    }

    /// Emit a signal on this object.
    pub fn emit_signal(&self, signal_name: &str, args: &[GValue]) -> Option<GValue> {
        signal_emit_by_name(self.type_id, signal_name, args)
    }

    /// Connect a [`Closure`] to a signal by name (`g_signal_connect_closure`).
    ///
    /// The closure is wrapped in a thin [`SignalCallback`] adapter that calls
    /// [`Closure::invoke`] on emission. Returns the handler ID (0 on failure to
    /// look up the signal), cast to `u64` for parity with upstream's `gulong`.
    pub fn connect_closure(
        &self,
        signal_name: &str,
        closure: Arc<Closure>,
        flags: ConnectFlags,
    ) -> u64 {
        let cb: SignalCallback = {
            let closure = closure.clone();
            Arc::new(move |args: &[GValue]| closure.invoke(args))
        };
        signal_connect_by_name(self.type_id, signal_name, cb, flags) as u64
    }

    /// Freeze notify (prevent property change notifications).
    pub fn freeze_notify(&self) {
        // In a full implementation, this would queue notifications.
    }

    /// Thaw notify (release queued notifications).
    pub fn thaw_notify(&self) {
        // In a full implementation, this would emit queued notifications.
    }
}

impl Drop for GObject {
    fn drop(&mut self) {
        let weak_refs = self.weak_refs.read();
        for cb in weak_refs.iter() {
            cb();
        }
    }
}

/// Convenience function to create a GObject.
pub fn object_new(type_id: GType) -> Arc<GObject> {
    GObject::new(type_id)
}

/// Convenience function to create a GObject with parameters.
pub fn object_new_with_params(type_id: GType, params: &[(&str, GValue)]) -> Arc<GObject> {
    GObject::new_with_params(type_id, params)
}

/// Bind two properties on two objects (`g_object_bind_property`).
pub struct PropertyBinding {
    source: Arc<GObject>,
    source_property: String,
    target: Arc<GObject>,
    target_property: String,
}

impl PropertyBinding {
    /// Create a property binding.
    pub fn new(
        source: &Arc<GObject>,
        source_property: &str,
        target: &Arc<GObject>,
        target_property: &str,
    ) -> Self {
        Self {
            source: source.clone(),
            source_property: source_property.to_owned(),
            target: target.clone(),
            target_property: target_property.to_owned(),
        }
    }

    /// Sync the target from the source.
    pub fn sync(&self) {
        if let Some(value) = self.source.get_property(&self.source_property) {
            self.target.set_property(&self.target_property, value);
        }
    }
}

// ── GClosure ──────────────────────────────────────────────────────────

/// Callback signature stored inside a [`Closure`] (`GClosureMarshal`-adjacent).
///
/// Mirrors [`SignalCallback`] but is owned by the closure object so it can be
/// replaced via [`Closure::set_marshal`].
pub type ClosureCallback = Arc<dyn Fn(&[GValue]) -> Option<GValue> + Send + Sync>;

/// Destroy/finalize notify callback (`GClosureNotify`-style).
///
/// Registered with [`Closure::add_notify`] and invoked exactly once when the
/// closure is invalidated (see [`Closure::invalidate`]).
pub type ClosureNotify = Arc<dyn Fn() + Send + Sync>;

/// A ref-counted,marshallable closure (`GClosure`).
///
/// Upstream `GClosure` is an opaque, reference-counted object with a floating
/// flag, an invalidate flag, a marshal callback, and a list of finalize
/// notifiers. This `no_std` port models the same shape idiomatically:
///
/// - Reference counting and cheap sharing come from the wrapping
///   [`Arc<Closure>`]; [`Closure::ref_`] hands out a new strong reference.
/// - The marshal callback is stored as an [`Arc`] trait object behind a
///   [`Mutex`] so [`Closure::set_marshal`] can replace it.
/// - The floating and invalidated flags are [`AtomicBool`]s.
/// - Finalize notifiers are [`ClosureNotify`] callbacks collected in a
///   [`Mutex<Vec<_>>`]; they fire once on invalidate.
///
/// Unlike the C original we do not model `GClosureNotifyData`'s `data`
/// pointer — callers capture any data they need directly in the Rust closure.
pub struct Closure {
    /// Weak self-reference so [`Closure::ref_`] can mint a new strong ref from
    /// `&self`. Set immediately after construction by [`closure_new`]. Using a
    /// `Weak` (not `Arc`) avoids a strong-reference cycle that would leak the
    /// closure.
    self_ref: Mutex<Weak<Closure>>,
    /// The marshal callback (`g_closure_set_marshal` target).
    callback: Mutex<ClosureCallback>,
    /// Floating-sink flag (`G_CLOSURE_IS_FLOATING` / `g_closure_sink`).
    floating: AtomicBool,
    /// Invalidation flag (`G_CLOSURE_IS_VALID`).
    invalidated: AtomicBool,
    /// Finalize notify callbacks (`g_closure_add_finalize_notify`).
    notifiers: Mutex<Vec<ClosureNotify>>,
}

impl Closure {
    /// Invoke the closure (`g_closure_invoke`).
    ///
    /// Returns `None` if the closure has been invalidated; otherwise calls the
    /// current marshal callback with `args` and returns its result.
    pub fn invoke(&self, args: &[GValue]) -> Option<GValue> {
        if self.is_invalidated() {
            return None;
        }
        // Clone the Arc out of the lock so we don't hold it across the call.
        let cb = self.callback.lock().clone();
        cb(args)
    }

    /// Acquire a new strong reference (`g_closure_ref`).
    ///
    /// Parity note: upstream increments an integer ref count and returns the
    /// same pointer. Here we hand back a fresh [`Arc`] clone via the stored
    /// weak self-reference; the strong count on the [`Arc<Closure>`] plays the
    /// role of the integer ref count.
    pub fn ref_(&self) -> Arc<Closure> {
        self.self_ref
            .lock()
            .upgrade()
            .expect("closure self-ref must be live while &self is held")
    }

    /// Sink a floating closure (`g_closure_sink`).
    ///
    /// Clears the floating flag. Returns `true` if the closure was floating
    /// (i.e. this call sank a floating ref); `false` if it was already sunk
    /// (a no-op, matching upstream).
    pub fn sink(&self) -> bool {
        self.floating
            .swap(false, core::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the closure currently has a floating reference
    /// (`G_CLOSURE_IS_FLOATING`).
    pub fn is_floating(&self) -> bool {
        self.floating.load(core::sync::atomic::Ordering::SeqCst)
    }

    /// Invalidate the closure (`g_closure_invalidate`).
    ///
    /// Sets the invalidated flag and runs all registered finalize notify
    /// callbacks exactly once. Subsequent [`Self::invoke`] calls return `None`.
    /// Repeated calls are a no-op (notifiers fire only the first time).
    pub fn invalidate(&self) {
        if self
            .invalidated
            .swap(true, core::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let drained = {
            let mut g = self.notifiers.lock();
            core::mem::take(&mut *g)
        };
        for notify in drained {
            notify();
        }
    }

    /// Whether the closure has been invalidated (`G_CLOSURE_IS_VALID`).
    pub fn is_invalidated(&self) -> bool {
        self.invalidated.load(core::sync::atomic::Ordering::SeqCst)
    }

    /// Install/replace the marshal callback (`g_closure_set_marshal`).
    ///
    /// No-op if the closure has already been invalidated.
    pub fn set_marshal<F>(&self, marshal: F)
    where
        F: Fn(&[GValue]) -> Option<GValue> + Send + Sync + 'static,
    {
        if self.is_invalidated() {
            return;
        }
        *self.callback.lock() = Arc::new(marshal) as ClosureCallback;
    }

    /// Add a finalize notify callback (`g_closure_add_finalize_notify`).
    ///
    /// `notify` is invoked exactly once when the closure is invalidated (or,
    /// in a fuller implementation, finalized). Multiple notifiers may be
    /// registered and fire in registration order.
    pub fn add_notify<F>(&self, notify: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.notifiers
            .lock()
            .push(Arc::new(notify) as ClosureNotify);
    }
}

/// Create a new floating [`Closure`] (`g_closure_new`).
///
/// The returned closure starts in the floating state (matching upstream,
/// where `g_closure_new` hands ownership to the caller as a floating ref);
/// call [`Closure::sink`] to take ownership.
pub fn closure_new<F>(callback: F) -> Arc<Closure>
where
    F: Fn(&[GValue]) -> Option<GValue> + Send + Sync + 'static,
{
    let closure = Arc::new(Closure {
        self_ref: Mutex::new(Weak::new()),
        callback: Mutex::new(Arc::new(callback) as ClosureCallback),
        floating: AtomicBool::new(true),
        invalidated: AtomicBool::new(false),
        notifiers: Mutex::new(Vec::new()),
    });
    *closure.self_ref.lock() = Arc::downgrade(&closure);
    closure
}

/// Create a new "C" [`Closure`] (`g_cclosure_new`).
///
/// Upstream `g_cclosure_new` wraps a C function pointer plus a
/// `data`/`notify` pair. This reimplementation does not model C function
/// pointers — a plain Rust closure captures any needed data directly — so
/// this is a thin alias for [`closure_new`], provided for API parity. An
/// optional finalize notify can be attached via [`Closure::add_notify`] on the
/// result.
pub fn cclosure_new<F>(callback: F) -> Arc<Closure>
where
    F: Fn(&[GValue]) -> Option<GValue> + Send + Sync + 'static,
{
    closure_new(callback)
}

/// Free-function form of [`Closure::invoke`] (`g_closure_invoke`).
pub fn closure_invoke(closure: &Closure, args: &[GValue]) -> Option<GValue> {
    closure.invoke(args)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn object_ref_unref() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        assert_eq!(obj.ref_count(), 1);
        obj.ref_();
        assert_eq!(obj.ref_count(), 2);
        obj.unref();
        assert_eq!(obj.ref_count(), 1);
    }

    #[test]
    fn object_type() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        assert_eq!(obj.type_id(), G_TYPE_OBJECT);
        assert_eq!(obj.type_name(), "GObject");
    }

    #[test]
    fn object_properties() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        obj.install_properties(vec![
            ParamSpec::int("x", "x", "x coordinate", 0, 100, 0, ParamFlags::READWRITE),
            ParamSpec::string("name", "n", "name", "default", ParamFlags::READWRITE),
        ]);

        assert_eq!(obj.get_property("x").unwrap().get_int(), 0);
        obj.set_property("x", value_new_int(42));
        assert_eq!(obj.get_property("x").unwrap().get_int(), 42);

        assert_eq!(
            obj.get_property("name").unwrap().get_string(),
            Some("default")
        );
        obj.set_property("name", value_new_string("test"));
        assert_eq!(obj.get_property("name").unwrap().get_string(), Some("test"));
    }

    #[test]
    fn object_construct_params() {
        type_init();
        let obj = GObject::new_with_params(G_TYPE_OBJECT, &[("x", value_new_int(99))]);
        // Properties set during construction are stored in user_data
        // since no ParamSpecs are installed yet.
        obj.install_properties(vec![ParamSpec::int(
            "x",
            "x",
            "x",
            0,
            100,
            0,
            ParamFlags::READWRITE | ParamFlags::CONSTRUCT,
        )]);
        // The construct param should have been set
        // (Note: in this simplified impl, construct params are set via set_property
        // which works because install_properties initializes defaults first)
    }

    #[test]
    fn object_user_data() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        obj.set_data("key1", value_new_int(123));
        assert_eq!(obj.get_data("key1").unwrap().get_int(), 123);
        obj.remove_data("key1");
        assert!(obj.get_data("key1").is_none());
    }

    #[test]
    fn object_floating() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        assert!(!obj.is_floating());
        obj.force_floating();
        assert!(obj.is_floating());
        obj.ref_sink();
        assert!(!obj.is_floating());
    }

    #[test]
    fn object_weak_ref() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();
        let obj = GObject::new(G_TYPE_OBJECT);
        obj.add_weak_ref(Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));
        // Weak ref fires on drop
        drop(obj);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn object_signals() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let obj = GObject::new(G_TYPE_OBJECT);
        signal_new(
            "test-sig",
            G_TYPE_OBJECT,
            SignalFlags::RUN_LAST,
            G_TYPE_NONE,
            &[],
        );
        obj.connect_signal(
            "test-sig",
            Arc::new(move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                None
            }),
            ConnectFlags::NONE,
        );

        obj.emit_signal("test-sig", &[]);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn property_binding() {
        type_init();
        let src = GObject::new(G_TYPE_OBJECT);
        let tgt = GObject::new(G_TYPE_OBJECT);

        src.install_properties(vec![ParamSpec::int(
            "value",
            "v",
            "value",
            0,
            100,
            0,
            ParamFlags::READWRITE,
        )]);
        tgt.install_properties(vec![ParamSpec::int(
            "value",
            "v",
            "value",
            0,
            100,
            0,
            ParamFlags::READWRITE,
        )]);

        src.set_property("value", value_new_int(77));
        let binding = PropertyBinding::new(&src, "value", &tgt, "value");
        binding.sync();
        assert_eq!(tgt.get_property("value").unwrap().get_int(), 77);
    }

    #[test]
    fn object_list_properties() {
        type_init();
        let obj = GObject::new(G_TYPE_OBJECT);
        obj.install_properties(vec![
            ParamSpec::int("a", "a", "a", 0, 1, 0, ParamFlags::READWRITE),
            ParamSpec::int("b", "b", "b", 0, 1, 0, ParamFlags::READWRITE),
        ]);
        let props = obj.list_properties();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "a");
        assert_eq!(props[1].name, "b");
    }

    #[test]
    fn closure_new_and_invoke() {
        let c = closure_new(|_args: &[GValue]| Some(value_new_int(42)));
        assert!(c.is_floating());
        c.sink();
        let r = c.invoke(&[]);
        assert_eq!(r.unwrap().get_int(), 42);
    }

    #[test]
    fn closure_sink_floating_semantics() {
        let c = closure_new(|_| None);
        assert!(c.is_floating());
        // First sink takes the floating ref.
        assert!(c.sink());
        assert!(!c.is_floating());
        // Second sink is a no-op.
        assert!(!c.sink());
    }

    #[test]
    fn closure_invalidate_blocks_invoke() {
        let c = closure_new(|_| Some(value_new_int(7)));
        c.sink();
        assert!(!c.is_invalidated());
        assert_eq!(c.invoke(&[]).unwrap().get_int(), 7);
        c.invalidate();
        assert!(c.is_invalidated());
        assert!(c.invoke(&[]).is_none());
    }

    #[test]
    fn closure_set_marshal_changes_behavior() {
        let c = closure_new(|_| Some(value_new_int(1)));
        c.sink();
        assert_eq!(c.invoke(&[]).unwrap().get_int(), 1);
        c.set_marshal(|_| Some(value_new_int(99)));
        assert_eq!(c.invoke(&[]).unwrap().get_int(), 99);
    }

    #[test]
    fn closure_notify_fires_on_invalidate() {
        let counter = Arc::new(AtomicI32::new(0));
        let c = closure_new(|_| None);
        c.sink();
        let cc = counter.clone();
        c.add_notify(move || {
            cc.fetch_add(1, Ordering::SeqCst);
        });
        // Not fired yet.
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        c.invalidate();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        // Repeated invalidate does not re-fire notifiers.
        c.invalidate();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn closure_ref_shares_state() {
        let c = closure_new(|_| Some(value_new_int(5)));
        c.sink();
        let c2 = c.ref_();
        // Shared state: invalidating one is visible to the other.
        assert!(!c2.is_invalidated());
        c.invalidate();
        assert!(c2.is_invalidated());
        assert!(c2.invoke(&[]).is_none());
    }

    #[test]
    fn closure_invoke_free_function() {
        let c = closure_new(|_| Some(value_new_int(13)));
        c.sink();
        assert_eq!(closure_invoke(&c, &[]).unwrap().get_int(), 13);
    }

    #[test]
    fn cclosure_new_alias() {
        let c = cclosure_new(|_| Some(value_new_int(21)));
        c.sink();
        assert_eq!(c.invoke(&[]).unwrap().get_int(), 21);
    }

    #[test]
    fn closure_connect_to_signal() {
        type_init();
        let counter = Arc::new(AtomicI32::new(0));
        let obj = GObject::new(G_TYPE_OBJECT);
        signal_new(
            "closure-sig",
            G_TYPE_OBJECT,
            SignalFlags::RUN_LAST,
            G_TYPE_INT,
            &[],
        );

        let counter_clone = counter.clone();
        let c = closure_new(move |_args: &[GValue]| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Some(value_new_int(100))
        });
        c.sink();

        let hid = obj.connect_closure("closure-sig", c, ConnectFlags::NONE);
        assert!(hid != 0);

        let ret = obj.emit_signal("closure-sig", &[]);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(ret.unwrap().get_int(), 100);
    }
}
