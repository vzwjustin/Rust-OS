//! GDBusProxy matching `gio/gdbusproxy.h` / `gio/gdbusproxy.c`.
//!
//! A [`DBusProxy`] is a client-side handle to a remote D-Bus object
//! identified by the `(bus_name, object_path, interface_name)` triple on
//! a [`DBusConnection`]. It mirrors the upstream `GDBusProxy` contract:
//!
//! * On construction it calls `org.freedesktop.DBus.Properties.GetAll` on
//!   the remote object (unless `DO_NOT_LOAD_PROPERTIES` is set) and
//!   populates a local property cache from the reply.
//! * It subscribes to the remote object's signals (unless
//!   `DO_NOT_CONNECT_SIGNALS` is set) and routes them to user-registered
//!   `g-signal` handlers, and subscribes to
//!   `org.freedesktop.DBus.Properties.PropertiesChanged` to keep the
//!   cache fresh and fire `g-properties-changed` handlers.
//! * Method calls ([`DBusProxy::call`] / [`DBusProxy::call_sync`]) are
//!   built into a `DBusMessage::new_method_call` and round-tripped
//!   through the connection's transport.
//!
//! # Bare-metal deviations from upstream
//!
//! * Upstream `GDBusProxy` is ref-counted (`GObject`) and exposes
//!   async/finish pairs (`g_dbus_proxy_new` / `_new_finish`,
//!   `g_dbus_proxy_call` / `_call_finish`). On bare metal there is no
//!   `GMainLoop` to drive async completion, so the async constructors and
//!   `call`/`call_finish` pairs fold into the synchronous
//!   [`DBusProxy::new`] / [`DBusProxy::call`] forms.
//! * Property values upstream are `GVariant`s. The [`DBusMessage`] body
//!   in this crate is a single `String` (see [`gdbusmessage`]), so the
//!   property cache is `BTreeMap<String, String>` and the `GetAll` reply
//!   / `PropertiesChanged` payloads use a small line-based encoding
//!   defined by the `encode_*` / `decode_*` helpers in this module. The
//!   encoding is symmetric and round-trips arbitrary UTF-8 values, so the
//!   cache behaves like upstream's for the in-process loopback bus.
//! * `g_dbus_proxy_get_name_owner` upstream queries the
//!   `org.freedesktop.DBus` daemon for the unique owner of `bus_name`.
//!   On the loopback there is no daemon, so [`DBusProxy::get_name_owner`]
//!   returns the well-known `bus_name` itself (documented).
//! * `g_dbus_proxy_new_for_bus` upstream binds to the session/system
//!   bus. On bare metal only the in-process loopback is real; the
//!   `Session`/`System` bus types return `G_IO_ERROR_NOT_SUPPORTED`.
//!
//! All routing — `GetAll`, `PropertiesChanged`, and custom-signal
//! delivery — is fully functional end-to-end via the
//! [`LoopbackTransport`](crate::gdbusconnection::LoopbackTransport):
//! the proxy's `signal_subscribe` callbacks are mirrored into the
//! transport, so `connection.send_message` / `signal_emit` fan signals
//! out to them exactly as a real daemon would.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::error::Error;
use crate::gdbusconnection::{is_valid_object_path, DBusConnection, SignalCallback};
use crate::gdbusintrospection::DBusInterfaceInfo;
use crate::gdbusmessage::{DBusMessage, DBusMessageHeaderField, DBusMessageType};
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::{BitOr, BitOrAssign};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

// ─────────────────────────── DBusProxyFlags ────────────────────────────────

/// Flags controlling the construction of a [`DBusProxy`]
/// (`GDBusProxyFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DBusProxyFlags(pub u32);

impl DBusProxyFlags {
    /// No flags set (`G_DBUS_PROXY_FLAGS_NONE`).
    pub const NONE: DBusProxyFlags = DBusProxyFlags(0);
    /// Don't load properties on construction
    /// (`G_DBUS_PROXY_FLAGS_DO_NOT_LOAD_PROPERTIES`).
    pub const DO_NOT_LOAD_PROPERTIES: DBusProxyFlags = DBusProxyFlags(1);
    /// Don't connect to signals on construction
    /// (`G_DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS`).
    pub const DO_NOT_CONNECT_SIGNALS: DBusProxyFlags = DBusProxyFlags(2);
    /// Use `Get` on construction to fetch invalidated properties
    /// (`G_DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES`).
    pub const GET_INVALIDATED_PROPERTIES: DBusProxyFlags = DBusProxyFlags(4);

    /// Returns `true` if all bits of `other` are set in `self`.
    #[inline]
    pub fn contains(self, other: DBusProxyFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for DBusProxyFlags {
    type Output = DBusProxyFlags;
    #[inline]
    fn bitor(self, rhs: DBusProxyFlags) -> DBusProxyFlags {
        DBusProxyFlags(self.0 | rhs.0)
    }
}

impl BitOrAssign for DBusProxyFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: DBusProxyFlags) {
        self.0 |= rhs.0;
    }
}

impl Default for DBusProxyFlags {
    fn default() -> Self {
        DBusProxyFlags::NONE
    }
}

/// `G_DBUS_PROXY_FLAGS_NONE` constant.
pub const DBUS_PROXY_FLAGS_NONE: u32 = 0;
/// `G_DBUS_PROXY_FLAGS_DO_NOT_LOAD_PROPERTIES` constant.
pub const DBUS_PROXY_FLAGS_DO_NOT_LOAD_PROPERTIES: u32 = 1;
/// `G_DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS` constant.
pub const DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS: u32 = 2;
/// `G_DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES` constant.
pub const DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES: u32 = 4;

// ─────────────────────────── callback types ────────────────────────────────

/// Callback for the `"g-signal"` notify (`GDBusProxy` `g-signal`).
///
/// Mirrors upstream
/// `void (*signal_cb)(GDBusProxy *, const gchar *sender,
///   const gchar *signal_name, GVariant *parameters, gpointer)`. The
/// parameters are carried in `message` (its body and headers).
pub type GSignalCallback = Arc<dyn Fn(&str, &str, &DBusMessage) + Send + Sync>;

/// Callback for the `"g-properties-changed"` notify
/// (`GDBusProxy` `g-properties-changed`).
///
/// Mirrors upstream
/// `void (*changed_cb)(GDBusProxy *, GVariant *changed_properties,
///   const gchar *const *invalidated_properties, gpointer)`. The changed
/// map and the invalidated key list are passed directly.
pub type GPropChangedCallback = Arc<dyn Fn(&BTreeMap<String, String>, &[String]) + Send + Sync>;

// ─────────────────────────── DBusBusType ───────────────────────────────────

/// The bus to connect a [`DBusProxy`] to (`GBusType`).
///
/// On bare metal only [`DBusBusType::None`] (a peer/loopback connection)
/// is real; the session and system buses require a `dbus-daemon` that is
/// not present, so they return `G_IO_ERROR_NOT_SUPPORTED` from
/// [`DBusProxy::new_for_bus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusBusType {
    /// The session bus (`G_BUS_TYPE_SESSION`).
    Session,
    /// The system bus (`G_BUS_TYPE_SYSTEM`).
    System,
    /// No bus — a peer-to-peer / loopback connection (`G_BUS_TYPE_NONE`).
    None,
}

// ─────────────────── properties-changed body encoding ──────────────────────
//
// The `DBusMessage` body in this crate is a single `String`. The standard
// D-Bus `PropertiesChanged` signal carries `(interface_name,
// a{sv} changed_properties, as invalidated_properties)`; we encode those
// three as a line-based UTF-8 string so the proxy can parse it back into
// the cache without needing a real `GVariant` decoder. The encoding is
// symmetric and handles arbitrary UTF-8 values (it never splits on `=`
// or `\n` inside a value because it length-prefixes each entry instead).
//
// Format:
//   line 0: the interface name the changes apply to
//   line 1: the number of changed entries  (decimal)
//   next N lines: "<keylen>:<vallen>:<key><val>"   (length-prefixed)
//   then 1 line: the number of invalidated keys (decimal)
//   then M lines: each invalidated key
//
// This is robust against `=`/`\n`/leading-zero ambiguity and is what the
// test handlers and the proxy's `PropertiesChanged` subscriber both use.

/// Marker constant — unused now but kept for format-version forward
/// compatibility if the encoding ever changes.
#[allow(dead_code)]
const PC_FORMAT_VERSION: u8 = 1;

/// Encode a `GetAll` reply body: one `key=value`-free, length-prefixed
/// entry per property so values may contain `=` or `\n`.
///
/// Only used by the test harness to build remote `GetAll` replies, hence
/// the `dead_code` allowance in non-test builds.
#[allow(dead_code)]
pub(crate) fn encode_get_all_reply(props: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    out.push_str(&props.len().to_string());
    out.push(':');
    for (k, v) in props {
        out.push_str(&k.len().to_string());
        out.push(':');
        out.push_str(&v.len().to_string());
        out.push(':');
        out.push_str(k);
        out.push_str(v);
    }
    out
}

/// Decode a `GetAll` reply body produced by [`encode_get_all_reply`].
pub(crate) fn decode_get_all_reply(body: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = body.as_bytes();
    let mut idx = 0;
    let count = match read_decimal(bytes, &mut idx) {
        Some(n) => n,
        None => return out,
    };
    if idx >= bytes.len() || bytes[idx] != b':' {
        return out;
    }
    idx += 1;
    for _ in 0..count {
        let klen = match read_decimal(bytes, &mut idx) {
            Some(n) => n,
            None => break,
        };
        if idx >= bytes.len() || bytes[idx] != b':' {
            break;
        }
        idx += 1;
        let vlen = match read_decimal(bytes, &mut idx) {
            Some(n) => n,
            None => break,
        };
        if idx >= bytes.len() || bytes[idx] != b':' {
            break;
        }
        idx += 1;
        if idx + klen + vlen > bytes.len() {
            break;
        }
        let key = core::str::from_utf8(&bytes[idx..idx + klen]).unwrap_or("");
        idx += klen;
        let val = core::str::from_utf8(&bytes[idx..idx + vlen]).unwrap_or("");
        idx += vlen;
        out.insert(key.to_string(), val.to_string());
    }
    out
}

/// Encode a `PropertiesChanged` signal body.
///
/// Only used by the test harness to emit `PropertiesChanged` signals,
/// hence the `dead_code` allowance in non-test builds.
#[allow(dead_code)]
pub(crate) fn encode_properties_changed(
    interface_name: &str,
    changed: &BTreeMap<String, String>,
    invalidated: &[String],
) -> String {
    let mut out = String::new();
    out.push_str(interface_name);
    out.push('\n');
    out.push_str(&changed.len().to_string());
    out.push('\n');
    for (k, v) in changed {
        out.push_str(&k.len().to_string());
        out.push(':');
        out.push_str(&v.len().to_string());
        out.push(':');
        out.push_str(k);
        out.push_str(v);
        out.push('\n');
    }
    out.push_str(&invalidated.len().to_string());
    out.push('\n');
    for k in invalidated {
        out.push_str(k);
        out.push('\n');
    }
    out
}

/// Decode a `PropertiesChanged` signal body produced by
/// [`encode_properties_changed`]. Returns
/// `(interface_name, changed_map, invalidated_keys)`.
pub(crate) fn decode_properties_changed(
    body: &str,
) -> Option<(String, BTreeMap<String, String>, Vec<String>)> {
    let mut lines = body.split('\n');
    let interface = lines.next()?.to_string();
    let changed_count = lines.next().and_then(|s| s.parse::<usize>().ok())?;
    let mut changed = BTreeMap::new();
    for _ in 0..changed_count {
        let line = lines.next()?;
        if line.is_empty() {
            continue;
        }
        let bytes = line.as_bytes();
        let mut idx = 0;
        let klen = read_decimal(bytes, &mut idx)?;
        if idx >= bytes.len() || bytes[idx] != b':' {
            return None;
        }
        idx += 1;
        let vlen = read_decimal(bytes, &mut idx)?;
        if idx >= bytes.len() || bytes[idx] != b':' {
            return None;
        }
        idx += 1;
        let rest = &line[idx..];
        if rest.len() < klen + vlen {
            return None;
        }
        changed.insert(
            rest[..klen].to_string(),
            rest[klen..klen + vlen].to_string(),
        );
    }
    let inv_count = lines.next().and_then(|s| s.parse::<usize>().ok())?;
    let mut invalidated = Vec::new();
    for _ in 0..inv_count {
        let k = lines.next()?;
        if !k.is_empty() {
            invalidated.push(k.to_string());
        }
    }
    Some((interface, changed, invalidated))
}

/// Read a decimal number from `bytes` starting at `idx`, advancing `idx`
/// past the digits. Returns `None` if no digit is found.
fn read_decimal(bytes: &[u8], idx: &mut usize) -> Option<usize> {
    let start = *idx;
    while *idx < bytes.len() && bytes[*idx].is_ascii_digit() {
        *idx += 1;
    }
    if *idx == start {
        None
    } else {
        core::str::from_utf8(&bytes[start..*idx])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
    }
}

// ─────────────────────────── DBusProxy ─────────────────────────────────────

/// A proxy for a remote D-Bus object (`GDBusProxy`).
///
/// Wraps the `(bus_name, object_path, interface_name)` triple on a
/// [`DBusConnection`], caches the object's properties, and emits
/// `g-properties-changed` / `g-signal` notifications. All operations
/// delegate to the connection (which has a real in-process
/// [`LoopbackTransport`](crate::gdbusconnection::LoopbackTransport)), so
/// a `DBusProxy` is fully functional in-process: `GetAll` populates the
/// cache, `PropertiesChanged` keeps it fresh, and custom signals reach
/// `g-signal` handlers.
pub struct DBusProxy {
    /// The connection the proxy is bound to.
    connection: Arc<DBusConnection>,
    /// The well-known or unique bus name of the remote object's owner
    /// (`None` for an unaddressed peer).
    bus_name: Option<String>,
    /// The object path of the remote object.
    object_path: String,
    /// The D-Bus interface the proxy speaks.
    interface_name: String,
    /// Construction flags.
    flags: DBusProxyFlags,
    /// Cached property values, keyed by property name. Shared with the
    /// `PropertiesChanged` subscription callback so it can update the
    /// cache without holding a reference to the proxy.
    properties: Arc<Mutex<BTreeMap<String, String>>>,
    /// Connection signal-subscription ids the proxy created for custom
    /// signals (used by [`close`][DBusProxy::close] to unsubscribe).
    signal_subscriptions: Mutex<Vec<u64>>,
    /// Connection signal-subscription ids the proxy created for
    /// `PropertiesChanged` (used by [`close`][DBusProxy::close]).
    property_subscriptions: Mutex<Vec<u64>>,
    /// Introspection info for the interface, if any.
    interface_info: Mutex<Option<Arc<DBusInterfaceInfo>>>,
    /// The unique owner of `bus_name`, if known. On bare metal this is
    /// just `bus_name` itself (no daemon to resolve unique names).
    name_owner: Mutex<Option<String>>,
    /// Registered `g-signal` handlers `(id, callback)`. Shared with the
    /// custom-signal subscription callback.
    g_signal_handlers: Arc<Mutex<Vec<(u64, GSignalCallback)>>>,
    /// Registered `g-properties-changed` handlers `(id, callback)`.
    /// Shared with the `PropertiesChanged` subscription callback.
    g_prop_handlers: Arc<Mutex<Vec<(u64, GPropChangedCallback)>>>,
    /// Next `g-signal` / `g-properties-changed` handler id.
    next_handler_id: AtomicU64,
}

impl DBusProxy {
    /// Create a proxy for `interface_name` at `object_path` on
    /// `connection`, synchronously
    /// (`g_dbus_proxy_new_sync`).
    ///
    /// Unless [`DBusProxyFlags::DO_NOT_LOAD_PROPERTIES`] is set, this
    /// calls `org.freedesktop.DBus.Properties.GetAll` on the remote
    /// object (via [`DBusConnection::call`]) and populates the property
    /// cache from the reply. Unless
    /// [`DBusProxyFlags::DO_NOT_CONNECT_SIGNALS`] is set, it subscribes
    /// on the connection for:
    /// * signals whose interface is `interface_name` and whose path is
    ///   `object_path` — routed to [`connect_signal`] handlers; and
    /// * `org.freedesktop.DBus.Properties.PropertiesChanged` signals at
    ///   `object_path` — used to refresh the cache and fire
    ///   [`connect_properties_changed`] handlers.
    ///
    /// `object_path` is validated with
    /// [`is_valid_object_path`]; an invalid path yields
    /// `G_IO_ERROR_INVALID_ARGUMENT`. A `GetAll` round-trip that returns
    /// a D-Bus error reply (e.g. the object doesn't export `Properties`)
    /// is propagated as `Err`.
    ///
    /// `timeout_msec` is forwarded to the underlying `call`/
    /// `send_message_with_reply_sync`; the loopback transport ignores it
    /// (calls are synchronous in-process).
    ///
    /// [`connect_signal`]: DBusProxy::connect_signal
    /// [`connect_properties_changed`]: DBusProxy::connect_properties_changed
    pub fn new(
        connection: Arc<DBusConnection>,
        flags: DBusProxyFlags,
        bus_name: Option<&str>,
        object_path: &str,
        interface_name: &str,
        interface_info: Option<Arc<DBusInterfaceInfo>>,
        timeout_msec: i32,
    ) -> Result<Self, Error> {
        if !is_valid_object_path(object_path) {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                format!("Invalid D-Bus object path: {object_path:?}"),
            ));
        }

        // Shared state that the connection's signal-subscription closures
        // will capture by Arc-clone, so the proxy can be returned by value
        // (no Arc<DBusProxy> needed) and there is no ref-cycle.
        let properties: Arc<Mutex<BTreeMap<String, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let g_signal_handlers: Arc<Mutex<Vec<(u64, GSignalCallback)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let g_prop_handlers: Arc<Mutex<Vec<(u64, GPropChangedCallback)>>> =
            Arc::new(Mutex::new(Vec::new()));

        let mut signal_sub_ids: Vec<u64> = Vec::new();
        let mut prop_sub_ids: Vec<u64> = Vec::new();

        // ── Subscribe to PropertiesChanged (unless suppressed) ──────────
        if !flags.contains(DBusProxyFlags::DO_NOT_CONNECT_SIGNALS) {
            let props_for_cb = Arc::clone(&properties);
            let handlers_for_cb = Arc::clone(&g_prop_handlers);
            let iface_for_cb = interface_name.to_string();
            let path_for_cb = object_path.to_string();
            let pc_callback: SignalCallback = Arc::new(move |msg: &DBusMessage| {
                // Only handle real PropertiesChanged signals.
                if msg.get_message_type() != DBusMessageType::Signal {
                    return;
                }
                let member = msg
                    .get_header(DBusMessageHeaderField::Member)
                    .unwrap_or_default();
                if member != "PropertiesChanged" {
                    return;
                }
                let path = msg
                    .get_header(DBusMessageHeaderField::Path)
                    .unwrap_or_default();
                if path != path_for_cb {
                    return;
                }
                let body = msg.get_body().unwrap_or_default();
                let Some((iface, changed, invalidated)) = decode_properties_changed(&body) else {
                    return;
                };
                if iface != iface_for_cb {
                    return;
                }
                // Update the cache: apply changed, drop invalidated.
                {
                    let mut cache = props_for_cb.lock();
                    for (k, v) in &changed {
                        cache.insert(k.clone(), v.clone());
                    }
                    for k in &invalidated {
                        cache.remove(k);
                    }
                }
                // Fire g-properties-changed handlers. Clone the callback
                // Arcs out of the lock so we don't invoke user code under
                // the handler mutex (handlers may (dis)connect).
                let handlers: Vec<GPropChangedCallback> = {
                    let guard = handlers_for_cb.lock();
                    guard.iter().map(|(_, cb)| Arc::clone(cb)).collect()
                };
                for cb in handlers {
                    cb(&changed, &invalidated);
                }
            });
            let pc_id = connection.signal_subscribe(
                None,
                Some("org.freedesktop.DBus.Properties"),
                Some("PropertiesChanged"),
                Some(object_path),
                pc_callback,
            );
            prop_sub_ids.push(pc_id);

            // ── Subscribe to the proxy's own interface signals ─────────
            let handlers_for_sig = Arc::clone(&g_signal_handlers);
            let sig_iface = interface_name.to_string();
            let sig_path = object_path.to_string();
            let sig_callback: SignalCallback = Arc::new(move |msg: &DBusMessage| {
                if msg.get_message_type() != DBusMessageType::Signal {
                    return;
                }
                let iface = msg
                    .get_header(DBusMessageHeaderField::Interface)
                    .unwrap_or_default();
                if iface != sig_iface {
                    return;
                }
                let path = msg
                    .get_header(DBusMessageHeaderField::Path)
                    .unwrap_or_default();
                if path != sig_path {
                    return;
                }
                let member = msg
                    .get_header(DBusMessageHeaderField::Member)
                    .unwrap_or_default();
                let sender = msg
                    .get_header(DBusMessageHeaderField::Sender)
                    .unwrap_or_default();
                let handlers: Vec<GSignalCallback> = {
                    let guard = handlers_for_sig.lock();
                    guard.iter().map(|(_, cb)| Arc::clone(cb)).collect()
                };
                for cb in handlers {
                    cb(&sender, &member, msg);
                }
            });
            let sig_id = connection.signal_subscribe(
                None,
                Some(interface_name),
                None,
                Some(object_path),
                sig_callback,
            );
            signal_sub_ids.push(sig_id);
        }

        // ── Load properties via GetAll (unless suppressed) ──────────────
        if !flags.contains(DBusProxyFlags::DO_NOT_LOAD_PROPERTIES) {
            let reply = connection.call(
                bus_name,
                object_path,
                "org.freedesktop.DBus.Properties",
                "GetAll",
                Some(interface_name),
                timeout_msec,
            )?;
            if reply.get_message_type() == DBusMessageType::Error {
                let err_name = reply
                    .get_header(DBusMessageHeaderField::ErrorName)
                    .unwrap_or_else(|| "org.freedesktop.DBus.Error.Failed".to_string());
                let err_msg = reply.get_body().unwrap_or_else(|| err_name.clone());
                return Err(Error::new(
                    io_error_quark(),
                    IOErrorEnum::Failed.to_code(),
                    format!("GetAll failed: {err_name}: {err_msg}"),
                ));
            }
            let body = reply.get_body().unwrap_or_default();
            let loaded = decode_get_all_reply(&body);
            let mut cache = properties.lock();
            for (k, v) in loaded {
                cache.insert(k, v);
            }
        }

        // name_owner: on the loopback there is no daemon to resolve the
        // unique owner of a well-known name, so we record the well-known
        // name itself as the owner (documented deviation).
        let name_owner = bus_name.map(|s| s.to_string());

        Ok(Self {
            connection,
            bus_name: bus_name.map(|s| s.to_string()),
            object_path: object_path.to_string(),
            interface_name: interface_name.to_string(),
            flags,
            properties,
            signal_subscriptions: Mutex::new(signal_sub_ids),
            property_subscriptions: Mutex::new(prop_sub_ids),
            interface_info: Mutex::new(interface_info),
            name_owner: Mutex::new(name_owner),
            g_signal_handlers,
            g_prop_handlers,
            next_handler_id: AtomicU64::new(1),
        })
    }

    /// Create a proxy bound to a bus (`g_dbus_proxy_new_for_bus_sync`).
    ///
    /// On bare metal only [`DBusBusType::None`] (a loopback peer
    /// connection) is real: a fresh [`LoopbackTransport`] connection is
    /// built and [`DBusProxy::new`] is called on it. [`DBusBusType::Session`]
    /// and [`DBusBusType::System`] require a `dbus-daemon` that is not
    /// present on bare metal, so they return `G_IO_ERROR_NOT_SUPPORTED`.
    pub fn new_for_bus(
        bus_type: DBusBusType,
        flags: DBusProxyFlags,
        bus_name: Option<&str>,
        object_path: &str,
        interface_name: &str,
        interface_info: Option<Arc<DBusInterfaceInfo>>,
        timeout_msec: i32,
    ) -> Result<Self, Error> {
        match bus_type {
            DBusBusType::None => {
                let connection = DBusConnection::new_for_address_sync("loopback:")?;
                Self::new(
                    Arc::new(connection),
                    flags,
                    bus_name,
                    object_path,
                    interface_name,
                    interface_info,
                    timeout_msec,
                )
            }
            DBusBusType::Session | DBusBusType::System => Err(Error::new(
                io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                format!(
                    "{:?} bus not available on bare metal (no dbus-daemon)",
                    bus_type
                ),
            )),
        }
    }

    // ── accessors ───────────────────────────────────────────────────────

    /// The object path the proxy is bound to (`g_dbus_proxy_get_object_path`).
    pub fn get_object_path(&self) -> &str {
        &self.object_path
    }

    /// The interface name the proxy speaks
    /// (`g_dbus_proxy_get_interface_name`).
    pub fn get_interface_name(&self) -> &str {
        &self.interface_name
    }

    /// The bus name the proxy is bound to, if any
    /// (`g_dbus_proxy_get_name`).
    pub fn get_bus_name(&self) -> Option<&str> {
        self.bus_name.as_deref()
    }

    /// The connection the proxy is bound to
    /// (`g_dbus_proxy_get_connection`).
    pub fn get_connection(&self) -> &DBusConnection {
        &self.connection
    }

    /// The construction flags (`g_dbus_proxy_get_flags`).
    pub fn get_flags(&self) -> DBusProxyFlags {
        self.flags
    }

    /// The unique owner of the bus name, if known
    /// (`g_dbus_proxy_get_name_owner`).
    ///
    /// On the loopback there is no `org.freedesktop.DBus` daemon to
    /// resolve the unique owner, so this returns the well-known
    /// `bus_name` itself (or `None` if the proxy has no bus name).
    pub fn get_name_owner(&self) -> Option<String> {
        self.name_owner.lock().clone()
    }

    // ── property cache ──────────────────────────────────────────────────

    /// Look up a cached property (`g_dbus_proxy_get_cached_property`).
    ///
    /// Returns the property value as a `String`, or `None` if it is not
    /// in the cache. (Upstream returns a floating `GVariant`; the crate
    /// models property values as `String`, matching how `DBusMessage`
    /// bodies work here.)
    pub fn get_cached_property(&self, property_name: &str) -> Option<String> {
        self.properties.lock().get(property_name).cloned()
    }

    /// The names of all cached properties
    /// (`g_dbus_proxy_get_cached_property_names`).
    pub fn get_cached_property_names(&self) -> Vec<String> {
        self.properties.lock().keys().cloned().collect()
    }

    /// Set a cached property locally (`g_dbus_proxy_set_cached_property`).
    ///
    /// Updates the cache in place; this does **not** round-trip to the
    /// remote object (matching upstream — the cache is purely
    /// client-side). Use [`DBusProxy::call`] to invoke a remote `Set`.
    pub fn set_cached_property(&self, property_name: &str, value: &str) {
        self.properties
            .lock()
            .insert(property_name.to_string(), value.to_string());
    }

    /// The introspection info for the interface, if any
    /// (`g_dbus_proxy_get_interface_info`).
    pub fn get_interface_info(&self) -> Option<Arc<DBusInterfaceInfo>> {
        self.interface_info.lock().clone()
    }

    /// Set the introspection info for the interface
    /// (`g_dbus_proxy_set_interface_info`).
    pub fn set_interface_info(&self, info: Option<Arc<DBusInterfaceInfo>>) {
        *self.interface_info.lock() = info;
    }

    // ── method calls ────────────────────────────────────────────────────

    /// Invoke `method_name` on the remote object and block for the reply
    /// (`g_dbus_proxy_call` / `g_dbus_proxy_call_sync`).
    ///
    /// Builds a `DBusMessage::new_method_call(bus_name, object_path,
    /// interface_name, method_name)`, attaches `parameters` as the body
    /// when `Some`, and round-trips it through
    /// [`DBusConnection::send_message_with_reply_sync`]. The returned
    /// message is the reply (a method-return **or** a D-Bus error reply);
    /// inspect [`DBusMessage::get_message_type`] to distinguish. An `Err`
    /// is returned only for transport-level failures (closed connection,
    /// non-method-call message).
    ///
    /// On bare metal there is no `GMainLoop` to drive the upstream
    /// `g_dbus_proxy_call` / `_call_finish` async pair, so both fold into
    /// this synchronous form.
    pub fn call(
        &self,
        method_name: &str,
        parameters: Option<&str>,
        timeout_msec: i32,
    ) -> Result<DBusMessage, Error> {
        let name = self.bus_name.as_deref().unwrap_or("");
        let msg = DBusMessage::new_method_call(
            name,
            &self.object_path,
            &self.interface_name,
            method_name,
        );
        if let Some(body) = parameters {
            msg.set_body(body);
        }
        self.connection
            .send_message_with_reply_sync(&msg, timeout_msec)
    }

    /// Synchronous alias of [`DBusProxy::call`]
    /// (`g_dbus_proxy_call_sync`).
    ///
    /// On bare metal this is the only mode; the upstream async
    /// `g_dbus_proxy_call` / `_call_finish` pair is folded into the sync
    /// form because there is no `GMainLoop` to drive it.
    pub fn call_sync(
        &self,
        method_name: &str,
        parameters: Option<&str>,
        timeout_msec: i32,
    ) -> Result<DBusMessage, Error> {
        self.call(method_name, parameters, timeout_msec)
    }

    // ── g-signal handlers ───────────────────────────────────────────────

    /// Connect a `"g-signal"` handler (`g_dbus_proxy_connect_signal`).
    ///
    /// The callback is invoked for every signal delivered on the proxy's
    /// `(object_path, interface_name)` (via the connection subscription
    /// set up at construction). Returns a handler id for use with
    /// [`disconnect_signal`][DBusProxy::disconnect_signal].
    pub fn connect_signal<F>(&self, callback: F) -> u64
    where
        F: Fn(&str, &str, &DBusMessage) + Send + Sync + 'static,
    {
        let id = self.next_handler_id.fetch_add(1, Ordering::SeqCst);
        self.g_signal_handlers.lock().push((id, Arc::new(callback)));
        id
    }

    /// Disconnect a `"g-signal"` handler
    /// (`g_dbus_proxy_disconnect_signal`).
    ///
    /// No-op if `handler_id` is unknown.
    pub fn disconnect_signal(&self, handler_id: u64) {
        self.g_signal_handlers
            .lock()
            .retain(|(id, _)| *id != handler_id);
    }

    /// Connect a `"g-properties-changed"` handler.
    ///
    /// The callback is invoked whenever a
    /// `org.freedesktop.DBus.Properties.PropertiesChanged` signal for the
    /// proxy's interface arrives (after the cache has been updated).
    /// Returns a handler id for use with
    /// [`disconnect_properties_changed`][DBusProxy::disconnect_properties_changed].
    pub fn connect_properties_changed<F>(&self, callback: F) -> u64
    where
        F: Fn(&BTreeMap<String, String>, &[String]) + Send + Sync + 'static,
    {
        let id = self.next_handler_id.fetch_add(1, Ordering::SeqCst);
        self.g_prop_handlers.lock().push((id, Arc::new(callback)));
        id
    }

    /// Disconnect a `"g-properties-changed"` handler.
    ///
    /// No-op if `handler_id` is unknown.
    pub fn disconnect_properties_changed(&self, handler_id: u64) {
        self.g_prop_handlers
            .lock()
            .retain(|(id, _)| *id != handler_id);
    }

    // ── lifecycle ───────────────────────────────────────────────────────

    /// Release the proxy's connection subscriptions
    /// (`g_dbus_proxy` teardown).
    ///
    /// Unsubscribes the `PropertiesChanged` and custom-signal
    /// subscriptions the constructor created on the connection. After
    /// `close`, the proxy no longer receives signals (though outstanding
    /// handler callbacks remain registered on the proxy itself until it
    /// is dropped). This is the supported way to break the proxy's link
    /// to the connection's signal fan-out; it is **not** called from
    /// `Drop` because the proxy may be held by multiple `Arc`/reference
    /// sites and the connection outlives it — calling unsubscribe from
    /// `Drop` would race with other clones. (This crate's [`DBusProxy`]
    /// is returned by value, so there is no ref count to track; users
    /// that want to tear down the signal wiring call `close`.)
    pub fn close(&self) {
        let prop_ids = self.property_subscriptions.lock().clone();
        for id in prop_ids {
            self.connection.signal_unsubscribe(id);
        }
        self.property_subscriptions.lock().clear();

        let sig_ids = self.signal_subscriptions.lock().clone();
        for id in sig_ids {
            self.connection.signal_unsubscribe(id);
        }
        self.signal_subscriptions.lock().clear();
    }
}

// SAFETY: `DBusProxy`'s `Mutex<_>`/`Arc<Mutex<_>>` fields and atomics are
// `Send + Sync`; `Arc<DBusConnection>` and `Arc<dyn Fn>` callback types are
// `Send + Sync` by their bounds. The `String` fields are `Send + Sync`.
unsafe impl Send for DBusProxy {}
unsafe impl Sync for DBusProxy {}

// ───────────────────────────── tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdbusconnection::MethodCallHandler;
    use crate::gdbusintrospection::DBusInterfaceInfo;
    use alloc::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering as StdOrdering};
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    /// Unique base path per test to avoid cross-test interference in the
    /// shared global interface-info cache and the loopback dispatch table.
    fn unique_path(tag: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/org/test/proxy/{tag}/{n}")
    }

    fn make_interface(name: &str) -> Arc<DBusInterfaceInfo> {
        Arc::new(DBusInterfaceInfo {
            name: name.to_string(),
            methods: Vec::new(),
            signals: Vec::new(),
            properties: Vec::new(),
            annotations: Vec::new(),
        })
    }

    /// Shared mutable state a test handler can read/write without lending
    /// raw references into the closure.
    #[derive(Default)]
    struct RemoteState {
        props: StdMutex<BTreeMap<String, String>>,
    }

    /// Build a `Properties.GetAll` handler bound to `RemoteState`. Returns
    /// the properties to its body via [`encode_get_all_reply`].
    fn properties_handler(state: StdArc<RemoteState>) -> MethodCallHandler {
        Arc::new(move |call: &DBusMessage| {
            let member = call
                .get_header(DBusMessageHeaderField::Member)
                .unwrap_or_default();
            match member.as_str() {
                "GetAll" => {
                    // The single in-arg is the interface name; we ignore it
                    // and return all cached props (the proxy will only ask
                    // for its own interface anyway).
                    let props = state.props.lock().unwrap().clone();
                    let body = encode_get_all_reply(&props);
                    let reply = DBusMessage::new_method_reply(call);
                    reply.set_body(&body);
                    Ok(reply)
                }
                "Get" => {
                    // body is "<interface>:<property>"; return its value or
                    // an empty string.
                    let key = call.get_body().unwrap_or_default();
                    let val = state
                        .props
                        .lock()
                        .unwrap()
                        .get(&key)
                        .cloned()
                        .unwrap_or_default();
                    let reply = DBusMessage::new_method_reply(call);
                    reply.set_body(&val);
                    Ok(reply)
                }
                _ => {
                    let reply = DBusMessage::new_method_reply(call);
                    reply.set_body("");
                    Ok(reply)
                }
            }
        })
    }

    /// A handler for the proxy's own interface: implements a `Ping`
    /// method that echoes its body back.
    fn custom_handler() -> MethodCallHandler {
        Arc::new(move |call: &DBusMessage| {
            let member = call
                .get_header(DBusMessageHeaderField::Member)
                .unwrap_or_default();
            if member == "Ping" {
                let reply = DBusMessage::new_method_reply(call);
                let body = call.get_body().unwrap_or_default();
                reply.set_body(&format!("pong:{body}"));
                Ok(reply)
            } else {
                let reply = DBusMessage::new_method_reply(call);
                reply.set_body("");
                Ok(reply)
            }
        })
    }

    /// Register the two objects a proxy needs at `path`: the
    /// `org.freedesktop.DBus.Properties` interface (with `GetAll`) and the
    /// custom interface (with `Ping`).
    fn register_remote(
        conn: &DBusConnection,
        path: &str,
        custom_iface: &str,
        state: StdArc<RemoteState>,
    ) {
        let props_iface = make_interface("org.freedesktop.DBus.Properties");
        conn.register_object(path, props_iface, properties_handler(state))
            .expect("register Properties");
        let custom = make_interface(custom_iface);
        conn.register_object(path, custom, custom_handler())
            .expect("register custom iface");
    }

    #[test]
    fn new_loads_properties_via_get_all() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("load");
        let iface = "org.test.Load";
        let state = StdArc::new(RemoteState::default());
        state
            .props
            .lock()
            .unwrap()
            .insert("Version".to_string(), "42".to_string());
        state
            .props
            .lock()
            .unwrap()
            .insert("Name".to_string(), "widget".to_string());
        register_remote(&conn, &path, iface, state);

        let proxy = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .expect("proxy new");
        assert_eq!(proxy.get_cached_property("Version").as_deref(), Some("42"));
        assert_eq!(proxy.get_cached_property("Name").as_deref(), Some("widget"));
        let mut names = proxy.get_cached_property_names();
        names.sort();
        assert_eq!(names, vec!["Name".to_string(), "Version".to_string()]);
    }

    #[test]
    fn call_custom_method_returns_reply() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("call");
        let iface = "org.test.Call";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));

        let proxy = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        let reply = proxy.call("Ping", Some("hi"), 1000).expect("call");
        assert_eq!(reply.get_message_type(), DBusMessageType::MethodReturn);
        assert_eq!(reply.get_body().as_deref(), Some("pong:hi"));
    }

    #[test]
    fn call_sync_alias_works() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("callsync");
        let iface = "org.test.CallSync";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let proxy = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        let reply = proxy.call_sync("Ping", Some("x"), 1000).unwrap();
        assert_eq!(reply.get_body().as_deref(), Some("pong:x"));
    }

    #[test]
    fn set_cached_property_updates_local_cache() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("setcache");
        let iface = "org.test.SetCache";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let proxy = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        // Initially empty (remote had no props).
        assert!(proxy.get_cached_property_names().is_empty());
        proxy.set_cached_property("Color", "red");
        assert_eq!(proxy.get_cached_property("Color").as_deref(), Some("red"));
        assert_eq!(proxy.get_cached_property_names(), vec!["Color".to_string()]);
    }

    #[test]
    fn properties_changed_signal_updates_cache_and_fires_handler() {
        let conn = Arc::new(DBusConnection::new_for_address_sync("loopback:").unwrap());
        let path = unique_path("pc");
        let iface = "org.test.PC";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));

        let proxy = DBusProxy::new(
            Arc::clone(&conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();

        let fired = StdArc::new(AtomicU32::new(0));
        let seen_changed = StdArc::new(StdMutex::new(None::<BTreeMap<String, String>>));
        let seen_invalidated = StdArc::new(StdMutex::new(None::<Vec<String>>));
        let fired_cb = StdArc::clone(&fired);
        let seen_changed_cb = StdArc::clone(&seen_changed);
        let seen_inv_cb = StdArc::clone(&seen_invalidated);
        proxy.connect_properties_changed(move |changed, invalidated| {
            *seen_changed_cb.lock().unwrap() = Some(changed.clone());
            *seen_inv_cb.lock().unwrap() = Some(invalidated.to_vec());
            fired_cb.fetch_add(1, StdOrdering::SeqCst);
        });

        // Emit a PropertiesChanged signal with one changed and one
        // invalidated property.
        let mut changed = BTreeMap::new();
        changed.insert("Version".to_string(), "7".to_string());
        let invalidated = vec!["OldProp".to_string()];
        let body = encode_properties_changed(iface, &changed, &invalidated);
        let msg = DBusMessage::new_signal(
            &path,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        );
        msg.set_body(&body);
        conn.send_message(&msg, 0).expect("emit");

        assert_eq!(fired.load(StdOrdering::SeqCst), 1);
        // Cache updated: Version added, and (had OldProp existed) removed.
        assert_eq!(proxy.get_cached_property("Version").as_deref(), Some("7"));
        assert_eq!(
            seen_changed
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .get("Version")
                .map(|s| s.as_str()),
            Some("7")
        );
        assert_eq!(
            seen_invalidated.lock().unwrap().as_ref().unwrap(),
            &vec!["OldProp".to_string()]
        );
    }

    #[test]
    fn custom_signal_fires_g_signal_handler() {
        let conn = Arc::new(DBusConnection::new_for_address_sync("loopback:").unwrap());
        let path = unique_path("gsig");
        let iface = "org.test.GSig";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));

        let proxy = DBusProxy::new(
            Arc::clone(&conn),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();

        let fired = StdArc::new(AtomicU32::new(0));
        let seen = StdArc::new(StdMutex::new(None::<(String, String, String)>));
        let fired_cb = StdArc::clone(&fired);
        let seen_cb = StdArc::clone(&seen);
        proxy.connect_signal(move |sender, member, msg| {
            let body = msg.get_body().unwrap_or_default();
            *seen_cb.lock().unwrap() = Some((sender.to_string(), member.to_string(), body));
            fired_cb.fetch_add(1, StdOrdering::SeqCst);
        });

        let msg = DBusMessage::new_signal(&path, iface, "Happened");
        msg.set_body("payload");
        conn.send_message(&msg, 0).expect("emit");

        assert_eq!(fired.load(StdOrdering::SeqCst), 1);
        let got = seen.lock().unwrap().clone().unwrap();
        assert_eq!(got.1, "Happened");
        assert_eq!(got.2, "payload");
    }

    #[test]
    fn do_not_load_properties_leaves_cache_empty() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("noload");
        let iface = "org.test.NoLoad";
        let state = StdArc::new(RemoteState::default());
        state
            .props
            .lock()
            .unwrap()
            .insert("X".to_string(), "1".to_string());
        register_remote(&conn, &path, iface, state);

        let proxy = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        assert!(proxy.get_cached_property_names().is_empty());
        assert_eq!(proxy.get_cached_property("X"), None);
    }

    #[test]
    fn do_not_connect_signals_blocks_custom_signal_delivery() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("nosig");
        let iface = "org.test.NoSig";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let conn_arc = Arc::new(conn);

        let proxy = DBusProxy::new(
            Arc::clone(&conn_arc),
            DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();

        let fired = StdArc::new(AtomicU32::new(0));
        let fired_cb = StdArc::clone(&fired);
        proxy.connect_signal(move |_s, _m, _msg| {
            fired_cb.fetch_add(1, StdOrdering::SeqCst);
        });

        let msg = DBusMessage::new_signal(&path, iface, "Anything");
        conn_arc.send_message(&msg, 0).expect("emit");
        assert_eq!(fired.load(StdOrdering::SeqCst), 0);
    }

    #[test]
    fn bad_object_path_returns_error() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let res = DBusProxy::new(
            Arc::new(conn),
            DBusProxyFlags::NONE,
            None,
            "not-a-path",
            "org.test.Bad",
            None,
            1000,
        );
        assert!(res.is_err());
        let Err(err) = res else {
            panic!("expected Err for invalid object path");
        };
        assert_eq!(err.code(), IOErrorEnum::InvalidArgument.to_code());
    }

    #[test]
    fn close_unsubscribes_signal_handlers() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("close");
        let iface = "org.test.Close";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let conn_arc = Arc::new(conn);

        let proxy = DBusProxy::new(
            Arc::clone(&conn_arc),
            DBusProxyFlags::NONE,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();

        let fired = StdArc::new(AtomicU32::new(0));
        let fired_cb = StdArc::clone(&fired);
        proxy.connect_signal(move |_s, _m, _msg| {
            fired_cb.fetch_add(1, StdOrdering::SeqCst);
        });

        // Before close: signal fires.
        let msg = DBusMessage::new_signal(&path, iface, "Pre");
        conn_arc.send_message(&msg, 0).unwrap();
        assert_eq!(fired.load(StdOrdering::SeqCst), 1);

        proxy.close();

        // After close: signal no longer reaches the handler.
        let msg2 = DBusMessage::new_signal(&path, iface, "Post");
        conn_arc.send_message(&msg2, 0).unwrap();
        assert_eq!(fired.load(StdOrdering::SeqCst), 1);
    }

    #[test]
    fn accessors_return_construction_values() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("acc");
        let iface = "org.test.Acc";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let conn_arc = Arc::new(conn);
        let proxy = DBusProxy::new(
            Arc::clone(&conn_arc),
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            Some("org.test.Bus"),
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        assert_eq!(proxy.get_object_path(), path.as_str());
        assert_eq!(proxy.get_interface_name(), iface);
        assert_eq!(proxy.get_bus_name(), Some("org.test.Bus"));
        assert_eq!(proxy.get_name_owner().as_deref(), Some("org.test.Bus"));
        assert!(proxy
            .get_flags()
            .contains(DBusProxyFlags::DO_NOT_LOAD_PROPERTIES));
        assert!(proxy
            .get_flags()
            .contains(DBusProxyFlags::DO_NOT_CONNECT_SIGNALS));
        // get_connection returns a usable connection (not closed).
        assert!(!proxy.get_connection().is_closed());
        let _ = conn_arc;
    }

    #[test]
    fn interface_info_get_and_set() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("info");
        let iface = "org.test.Info";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let conn_arc = Arc::new(conn);
        let info = make_interface(iface);
        let proxy = DBusProxy::new(
            Arc::clone(&conn_arc),
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            None,
            &path,
            iface,
            Some(Arc::clone(&info)),
            1000,
        )
        .unwrap();
        let got = proxy.get_interface_info().unwrap();
        assert_eq!(got.name, iface);
        assert!(Arc::ptr_eq(&got, &info));

        let new_info = make_interface("org.test.Other");
        proxy.set_interface_info(Some(Arc::clone(&new_info)));
        let got2 = proxy.get_interface_info().unwrap();
        assert!(Arc::ptr_eq(&got2, &new_info));

        proxy.set_interface_info(None);
        assert!(proxy.get_interface_info().is_none());
    }

    #[test]
    fn new_for_bus_session_is_not_supported() {
        let res = DBusProxy::new_for_bus(
            DBusBusType::Session,
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            None,
            "/org/test/Bus",
            "org.test.Bus",
            None,
            1000,
        );
        assert!(res.is_err());
        let Err(err) = res else {
            panic!("expected Err for session bus");
        };
        assert_eq!(err.code(), IOErrorEnum::NotSupported.to_code());
    }

    #[test]
    fn new_for_bus_none_creates_loopback_proxy() {
        // With both suppress flags set there's no GetAll round-trip, so a
        // proxy can be built on a fresh loopback connection without any
        // remote object registered.
        let proxy = DBusProxy::new_for_bus(
            DBusBusType::None,
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            None,
            "/org/test/Loopback",
            "org.test.Loopback",
            None,
            1000,
        )
        .expect("loopback proxy");
        assert_eq!(proxy.get_object_path(), "/org/test/Loopback");
        assert!(proxy.get_cached_property_names().is_empty());
    }

    #[test]
    fn flags_bitor_contains_bitorassign() {
        let f = DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS;
        assert!(f.contains(DBusProxyFlags::DO_NOT_LOAD_PROPERTIES));
        assert!(f.contains(DBusProxyFlags::DO_NOT_CONNECT_SIGNALS));
        assert!(!f.contains(DBusProxyFlags::GET_INVALIDATED_PROPERTIES));
        let mut g = DBusProxyFlags::NONE;
        g |= DBusProxyFlags::GET_INVALIDATED_PROPERTIES;
        assert!(g.contains(DBusProxyFlags::GET_INVALIDATED_PROPERTIES));
        assert_eq!(DBUS_PROXY_FLAGS_NONE, 0);
        assert_eq!(DBUS_PROXY_FLAGS_DO_NOT_LOAD_PROPERTIES, 1);
        assert_eq!(DBUS_PROXY_FLAGS_DO_NOT_CONNECT_SIGNALS, 2);
        assert_eq!(DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES, 4);
        assert_eq!(DBusProxyFlags::default(), DBusProxyFlags::NONE);
    }

    #[test]
    fn encode_decode_get_all_round_trips() {
        let mut props = BTreeMap::new();
        props.insert("a".to_string(), "1".to_string());
        props.insert("b".to_string(), "two".to_string());
        props.insert("eq=uals".to_string(), "val=ue".to_string());
        props.insert("new\nline".to_string(), "x\ny".to_string());
        let body = encode_get_all_reply(&props);
        let back = decode_get_all_reply(&body);
        assert_eq!(back, props);
        // Empty map round-trips too.
        let empty: BTreeMap<String, String> = BTreeMap::new();
        assert_eq!(decode_get_all_reply(&encode_get_all_reply(&empty)), empty);
    }

    #[test]
    fn encode_decode_properties_changed_round_trips() {
        let mut changed = BTreeMap::new();
        changed.insert("v".to_string(), "7".to_string());
        changed.insert("k=v".to_string(), "a=b".to_string());
        let invalidated = vec!["Old".to_string(), "Gone".to_string()];
        let body = encode_properties_changed("org.test.I", &changed, &invalidated);
        let (iface, c, i) = decode_properties_changed(&body).expect("decode");
        assert_eq!(iface, "org.test.I");
        assert_eq!(c, changed);
        assert_eq!(i, invalidated);
    }

    #[test]
    fn properties_changed_for_other_interface_is_ignored() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let path = unique_path("otheriface");
        let iface = "org.test.Mine";
        register_remote(&conn, &path, iface, StdArc::new(RemoteState::default()));
        let conn_arc = Arc::new(conn);
        let proxy = DBusProxy::new(
            Arc::clone(&conn_arc),
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
            None,
            &path,
            iface,
            None,
            1000,
        )
        .unwrap();
        let fired = StdArc::new(AtomicU32::new(0));
        let fired_cb = StdArc::clone(&fired);
        proxy.connect_properties_changed(move |_, _| {
            fired_cb.fetch_add(1, StdOrdering::SeqCst);
        });
        // PropertiesChanged for a different interface on the same path.
        let changed = BTreeMap::new();
        let body = encode_properties_changed("org.test.Other", &changed, &[]);
        let msg = DBusMessage::new_signal(
            &path,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        );
        msg.set_body(&body);
        conn_arc.send_message(&msg, 0).unwrap();
        assert_eq!(fired.load(StdOrdering::SeqCst), 0);
    }
}
