# glib-native RustOS Compatibility Layer Migration Plan

This document describes a phased strategy for building a Rust implementation of
core GLib-compatible primitives for RustOS. The goal is not to replace upstream
GLib immediately, but to incrementally implement GLib-like behavior in `no_std`
Rust while preserving a future path toward C ABI compatibility, Meson
integration, and broader GNOME/GObject support.

## Principles

1. **Incremental replacement** — Rust modules land alongside C sources; each phase
   replaces a leaf or well-bounded subsystem.
2. **Behavior parity** — Rust implementations are validated against existing GLib
   tests where possible, plus new Rust unit tests.
3. **FFI last** — Early phases are pure Rust crates under `rust/`. C interoperability
   (static libs, `cbindgen` headers, GObject type registration) comes when a
   subsystem is ready to be called from remaining C code.
4. **Bottom-up dependency order** — Convert modules with few or no internal GLib
   dependencies first.

## Repository layout

```
rust/
  Cargo.toml              # workspace root
  glib-native/              # Phase 1+ Rust implementations
    src/
      lib.rs
      endian.rs
      checked.rs
      refcount.rs
      bytes.rs
      ...
docs/rust-migration.md    # this file
```

The crate is named `glib-native` to avoid confusion with the existing
[gtk-rs glib](https://crates.io/crates/glib) bindings crate.

## Phase overview

| Phase | Scope | Key C sources | Status |
|-------|-------|---------------|--------|
| **0** | Tooling: Cargo workspace, `cargo test` in CI, migration docs | — | **Done** |
| **1** | Foundation types: endian swaps, checked arithmetic, refcounts, `GBytes`, ref-counted strings/boxes | `gtypes.h`, `grefcount.*`, `gbytes.*`, `grefstring.*`, `grcbox.*` | **Done** |
| **2** | Atomics, memory helpers, strings, random, printf, slice | `gatomic.*`, `gmem.*`, `gstrfuncs.*`, `gstring.*`, `grand.*`, `gprintf.*`, `gslice.*` (deprecated) | **Done** |
| **3** | Sequential containers | `garray.*`, `glist.*`, `gslist.*`, `gqueue.*`, `gptrarray.*`, `gnode.*`, `gsequence.*`, `gcompletion.*` (deprecated), `gqsort.*`, `gprimes.*` | **Done** |
| **4** | Associative containers & datasets | `ghash.*`, `gtree.*`, `gdataset.*`, `gquark.*`, `grel.*` (deprecated), `gcache.*` (deprecated) | **Done** |
| **5** | Errors, logging, options | `gerror.*`, `gmessages.*`, `goption.*` | **Done** |
| **6** | I/O primitives | `gfileutils.*`, `gconvert.*`, `gcharset.*`, `gchecksum.*`, `gbase64.*`, `ghmac.*`, `ghostutils.*`, `genviron.*`, `gkeyfile.*`, `gbitlock.*`, `ghook.*`, `gpattern.*`, `gshell.*`, `guri.*`, `gmarkup.*`, `gstringchunk.*`, `gstrvbuilder.*`, `gversion.*`, `gscanner.*`, `gtimer.*`, `gutils.*` (partial), `gpathbuf.*`, `guuid.*`, `gdir.*`, `gmappedfile.*`, `gregex.*`, `gspawn.*`, `gstdio.*`, `gtestutils.*` | **Partial** (pure logic done, OS-dependent ops (dir/spawn/stdio/mappedfile) are stubs with platform traits, regex has backtracking engine for basic patterns) |
| **7** | Date/time & variants | `gdate.*`, `gdatetime.*`, `gtimezone.*`, `gvarianttype.*`, `gvariant.*`, `gunicode.*` | **Partial** (gdate pure math, datetime UTC arithmetic/format, timezone fixed offsets, varianttype parser, variant value container + builder + parser, unicode types/enums + combining class, basic utf8/unichar done, IANA tz DB/datetime tz integration planned) |
| **8** | Main loop & threading | `gmain.*`, `gsource.*`, `gthread.*`, `gasyncqueue.*`, `gpoll.*`, `giochannel.*`, `gthreadpool.*` | **Partial** (async queue, thread primitives, poll types, I/O channel types, main loop types (MainContext/MainLoop/Source), timeout_add/idle_add, thread pool task queue done, actual thread creation/poll-based event loop needs OS support) |
| **9** | GObject core | `gobject/*` (types, signals, properties, values) | **Partial** (GType registry with 21 fundamental types, type registration/lookup/query, GValue polymorphic container for all basic types, GParamSpec with typed constructors, GSignal with connect/emit/disconnect, GObject base class with ref counting, properties, weak refs, user data, property binding) |
| **10** | GModule and platform runtime integration | `gmodule/*`, `gthread/*` | **Partial** (gmodule ported: `GModule` ref-counted handle, `ModulePlatform` trait for OS-specific dlopen/dlsym/dlclose, `NoModulePlatform` stub for bare metal, registry with name/handle dedup, `module_open_full`/`module_open`/`module_close`/`module_symbol`/`module_make_resident`/`module_build_path`/`module_error`/`module_error_quark`, 20 unit tests passing) |
| **11** | GIO (split into sub-phases: streams, sockets, D-Bus, settings, …) | `gio/*` | **Partial** (gfileattribute ported: `FileAttributeType` enum (10 types), `FileAttributeInfoFlags` (COPY_WITH_FILE/COPY_WHEN_MOVED), `FileAttributeInfo` struct, `FileAttributeInfoList` ref-counted sorted-by-name list with binary-search `lookup`/`add`/`dup`/`ref_`/`n_infos`/`infos`, 14 unit tests passing; gdbusintrospection ported: 7 ref-counted info structs (Annotation/Arg/Method/Signal/Property/Interface/Node) with `Arc<T>` ref counting, `DBusPropertyInfoFlags` (READABLE/WRITABLE), lookup helpers (`dbus_annotation_info_lookup`/`dbus_interface_info_lookup_method`/`_signal`/`_property`/`dbus_node_info_lookup_interface`), 12 unit tests passing; gdbuserror ported: `DBusError` enum (44 well-known `org.freedesktop.DBus.Error.*` codes), `DBusErrorEntry` struct, `dbus_error_quark` (lazily registers all 44 well-known entries), `dbus_error_register_error`/`_unregister_error`/`_register_error_domain` global registry (BTreeMap-backed), `dbus_error_is_remote_error`/`_get_remote_error`/`_strip_remote_error` remote-error prefix parsing, `dbus_error_new_for_dbus_error` (registered + `org.gtk.GDBus.UnmappedGError.Quark._*` fallback), `dbus_error_encode_gerror` (with hex-escaped `_XX` unmapped form), 23 unit tests passing; gioerror ported: `IOErrorEnum` enum (49 codes; `CONNECTION_CLOSED` const-aliased to `BrokenPipe` since Rust forbids duplicate discriminants), `io_error_quark`, `io_error_from_errno` (errno→IOErrorEnum via file_error_from_errno + additional socket/network codes), `io_error_from_file_error` (FileError→IOErrorEnum), 8 unit tests passing; gnotification ported: `Notification` struct (plain Rust port of upstream GObject subclass — fields title/body/icon/priority/category/buttons/default_action/default_action_target), `NotificationPriority` enum (Normal/Low/High/Urgent), `NotificationButton` struct (label/action_name/target: Option<Variant>), `NotificationIcon` opaque type (`Arc<dyn Any + Send + Sync>`) for deferred GIcon support, full setter API (`set_title`/`set_body`/`set_priority`/`set_urgent`/`set_category`/`add_button`/`add_button_with_target_value`/`set_default_action`/`set_default_action_with_target_value`/`set_icon`) + accessors, 16 unit tests passing; gsrvtarget ported: `SrvTarget` boxed struct (hostname/port/priority/weight) with `Clone`/`PartialEq`/`Eq`/`Hash`, `new`/`hostname`/`port`/`priority`/`weight` accessors, `srv_target_list_sort` implementing RFC 2782 priority+weight sorting (single-"."-hostname special case, priority-ascending sort, weighted-random selection within priority groups using `random_int_range`), 12 unit tests passing; ginetaddress ported: `SocketFamily` enum (Invalid/Unix/Ipv4/Ipv6 with Linux `AF_*` values 0/1/2/10), `InetAddress` plain-struct port of upstream GObject subclass with `InetAddrBytes` enum (Ipv4 [u8;4] / Ipv6 [u8;16]), `new_from_string` (hand-written IPv4 dotted-quad + IPv6 text parser with `::` compression and embedded IPv4), `new_from_bytes`/`new_loopback`/`new_any`/`equal`/`to_string`/`to_bytes`/`native_size`/`family`, full classification suite (`is_any`/`is_loopback`/`is_link_local`/`is_site_local`/`is_multicast`/`is_mc_global`/`is_mc_link_local`/`is_mc_node_local`/`is_mc_org_local`/`is_mc_site_local`), RFC 5952 `::` compression in `to_string`, 18 unit tests passing; fileutils gained `file_error_from_errno` (errno→FileError, needed by gioerror); remaining GIO submodules — streams, sockets, D-Bus connection, settings, GFile, GIcon, GCancellable, GAsyncResult, etc. — planned; XML parse/generate for introspection info deferred) |
| **12** | GObject Introspection & tools | `girepository/*`, `tools/*` | Planned |
| **13** | Remove C implementations; expose stable C ABI from Rust via `extern "C"` | all | Planned |

## `no_std` refactor

The `glib-native` crate has been refactored to be `#![no_std]` with `extern crate alloc`,
making it suitable for use in kernel environments (e.g. RustOS).

### Changes

- **`lib.rs`** — Added `#![no_std]`, `#[macro_use] extern crate alloc`, `#[cfg(test)] extern crate std`.
  A `prelude` module provides `String`, `Vec`, `Box`, `ToOwned`, `ToString` from `alloc`.
  A `gwarn!` macro is no-op on `no_std`, `eprintln!` under `#[cfg(test)]`.
- **Synchronization** — `std::sync::{Mutex, RwLock, OnceLock}` replaced with `spin` crate equivalents
  (`spin::mutex::Mutex`, `spin::rwlock::RwLock`, `spin::Once`). `spin` features: `spin_mutex`, `rwlock`, `once`.
- **Collections** — `std::collections::HashMap` replaced with `alloc::collections::BTreeMap`.
- **I/O** — `std::io` (stdout/stderr) replaced with no-op functions in `messages.rs`.
  Print handlers are pluggable via `set_print_handler` / `set_printerr_handler`.
- **Abort** — `std::process::abort` replaced with `panic!` (gated `#[cfg(not(test))]`).
- **All `std::` imports** replaced with `core::` or `alloc::` equivalents (`std::ffi`, `std::ptr`,
  `std::mem`, `std::cell`, `std::fmt`, `std::cmp`, `std::ops`, `std::slice`, `std::str`, etc.).
- **`std::sync::Arc`** replaced with `alloc::sync::Arc`.
- **Test code** retains `std::` usage behind `#[cfg(test)]` (e.g. `std::thread::spawn`,
  `std::panic::catch_unwind`, `std::ffi::CString`).

### RustOS integration

`glib-native` is wired as a path dependency in RustOS (`Cargo.toml`):
```toml
glib-native = { path = "../glib-rust/glib/rust/glib-native" }
```

A `glib` wrapper module (`src/glib.rs`) re-exports all `glib_native` types/functions and provides:
- `init_glib_logging()` — routes `g_print` / `g_printerr` through the kernel serial port.
- `smoke_check()` — boot-time integration validation exercising 60+ GLib primitives,
  including the new regex engine (compile, match with captures, split, replace),
  thread pool (push, unprocessed count), and test framework (TestSuite, TestCase).

All 46 ported modules have their public types, functions, and constants re-exported
in `rust-os/src/glib.rs` via `pub use glib_native::*` plus an explicit alphabetical
re-export list for documentation and name resolution.

## Phase 8 detail (partial)

### Modules

- **`asyncqueue`** — `AsyncQueue<T>` with `GMutex`/`GCond` for blocking pop.
- **`thread`** — `GMutex<T>`, `GRecMutex`, `GRWLock`, `GCond`, `Once` wrapping `spin` primitives.
- **`poll`** — `PollFD`, `IOCondition`, `PollFunc` types.
- **`iochannel`** — I/O channel types and enums.
- **`mainloop`** — `MainContext`, `MainLoop`, `Source` with `prepare`/`check`/`dispatch` callbacks,
  `timeout_add`, `idle_add`, `source_remove`.
- **`threadpool`** — `ThreadPool` with task queue (`push`, `process_all`, `move_to_front`,
  `set_sort_function`), global unused-thread settings (`set_max_unused_threads`, etc.).
  No actual OS thread creation — `process_all` runs queued tasks inline.

### Deferred

- Full poll-based event loop (needs OS `poll`/`epoll` syscall).
- Thread creation/joining (needs OS `clone`/`futex` or `pthread`).
- Real thread pool worker threads.

## Phase 11 detail (partial)

### Modules

- **`gfileattribute`** — First GIO submodule. Mirrors
  `gio/gfileattribute.h` / `gio/gfileattribute.c`:
  - `FileAttributeType` enum (10 types: Invalid, String, ByteString,
    Boolean, Uint32, Int32, Uint64, Int64, Object, Stringv) with
    `#[repr(u32)]` so the discriminant values match the upstream C
    enum. `as_str()` method matching
    `g_file_attribute_type_to_string`.
  - `FileAttributeInfoFlags` (NONE / COPY_WITH_FILE /
    COPY_WHEN_MOVED) with `BitOr` and `contains`.
  - `FileAttributeInfo` struct (name, type, flags).
  - `FileAttributeInfoList` — ref-counted (via `Arc`) sorted-by-name
    list with binary-search `lookup` and `add` (insert-or-update),
    `dup` (deep copy), `ref_` (clone for API parity), `n_infos` /
    `infos` / `info` accessors. Mirrors
    `g_file_attribute_info_list_new` / `_dup` / `_ref` / `_unref` /
    `_lookup` / `_add` and the internal
    `g_file_attribute_info_list_bsearch`. `add` requires unique
    ownership (ref count == 1); if shared, callers `dup` first and
    mutate the clone.
  - 14 unit tests covering enum values, flags, empty list, add +
    lookup, update-in-place, sorted insertion, dup independence, ref
    count, indexing, out-of-bounds panic, and binary-search
    correctness with 26 entries — all passing.

- **`gdbusintrospection`** — GIO D-Bus introspection info structs.
  Mirrors `gio/gdbusintrospection.h` / `gio/gdbusintrospection.c`:
  - `DBusPropertyInfoFlags` (NONE / READABLE / WRITABLE) with
    `BitOr` and `contains`.
  - 7 ref-counted info structs (upstream uses atomic-int ref counting;
    we use `Arc<T>`): `DBusAnnotationInfo` (key/value/nested
    annotations), `DBusArgInfo` (name/signature/annotations),
    `DBusMethodInfo` (name/in_args/out_args/annotations),
    `DBusSignalInfo` (name/args/annotations), `DBusPropertyInfo`
    (name/signature/flags/annotations), `DBusInterfaceInfo`
    (name/methods/signals/properties/annotations), `DBusNodeInfo`
    (path/interfaces/nodes/annotations). Each has a `ref_` method
    for API parity with the upstream `_ref` functions.
  - Lookup helpers with linear search (matching upstream's
    uncached behaviour): `dbus_annotation_info_lookup`,
    `dbus_interface_info_lookup_method` / `_signal` / `_property`,
    `dbus_node_info_lookup_interface`.
  - 12 unit tests covering flags, annotation lookup, method/signal/
    property/interface lookup, nested annotations, ref count, and
    full hierarchy construction + lookup — all passing.

- **`gdbuserror`** — GIO D-Bus error handling. Mirrors
  `gio/gdbuserror.h` / `gio/gdbuserror.c`:
  - `DBusError` enum (44 well-known `org.freedesktop.DBus.Error.*`
    codes) with `#[repr(i32)]` so discriminant values match the
    upstream C enum. `to_code()` and `to_dbus_name()` accessors.
  - `DBusErrorEntry` struct for registering error-domain tables.
  - `dbus_error_quark()` — lazily-initialized `G_DBUS_ERROR` quark
    that registers all 44 well-known entries via
    `dbus_error_register_error_domain` on first call (using
    `spin::once::Once`).
  - `dbus_error_register_error` / `_unregister_error` /
    `_register_error_domain` — global registry mapping
    `(Quark, code)` <-> `dbus_error_name`, backed by two
    `BTreeMap`s in a `spin::Mutex` (mirrors upstream's two hash
    tables). `register` returns `false` on duplicate pair or name.
  - `dbus_error_is_remote_error` / `_get_remote_error` /
    `_strip_remote_error` — recognize and handle the
    `"GDBus.Error:NAME: "` prefix. `_get_remote_error` first checks
    the registry by `(domain, code)`, then falls back to prefix
    parsing. `_strip_remote_error` mutates the `Error` in place via
    a new `Error::set_message` method.
  - `dbus_error_new_for_dbus_error` — build a `glib_native::Error`
    from a D-Bus error name + message. Uses registered `(domain,
    code)` if available, else decodes the
    `org.gtk.GDBus.UnmappedGError.Quark._*` form, else falls back
    to a synthetic domain quark.
  - `dbus_error_encode_gerror` — encode an `Error` as a D-Bus error
    name. Uses the registered name if available, else produces the
    `org.gtk.GDBus.UnmappedGError.Quark._<hex-escaped-quark-name>.Code<code>`
    form with `_XX` hex escapes for non-alphanumeric chars (matches
    upstream exactly so interop works).
  - `parse_remote_prefix` — public helper that parses the
    `"GDBus.Error:NAME: REST"` prefix (handles colons in the
    message body by finding the first `": "` separator).
  - 23 unit tests covering enum values, name/code accessors, quark
    stability, well-known entry registration, custom register/
    unregister semantics, domain registration, remote-error
    detection/parsing/stripping, new_for_dbus_error (registered +
    unmapped + fallback), encode round-trip with hex escapes,
    parse with colons in message — all passing.

- **`gioerror`** — GIO error codes. Mirrors `gio/gioerror.h` /
  `gio/gioerror.c`:
  - `IOErrorEnum` enum (49 codes: Failed, NotFound, Exists, ...,
    BrokenPipe, NotConnected, MessageTooLarge, NoSuchDevice,
    DestinationUnset) with `#[repr(i32)]` so discriminant values
    match the upstream C enum. `CONNECTION_CLOSED` is exposed as a
    `pub const` alias for `BrokenPipe` (upstream has
    `G_IO_ERROR_CONNECTION_CLOSED = G_IO_ERROR_BROKEN_PIPE`, but
    Rust forbids duplicate enum discriminants). `to_code()` accessor.
  - `io_error_quark()` — the `G_IO_ERROR` quark.
  - `io_error_from_file_error()` — `FileError` → `IOErrorEnum`
    mapping matching the upstream `switch` (including the
    `NoSpc`/`NoMem` → `NoSpace` and `MFile`/`NFile` →
    `TooManyOpenFiles` collapses, and the `BadF`/`Failed`/`Fault`/
    `Intr`/`Io` → `Failed` group).
  - `io_error_from_errno()` — errno → `IOErrorEnum` via
    `file_error_from_errno` + `io_error_from_file_error`, then a
    second-pass `switch` for socket/network errnos that don't have a
    `FileError` counterpart (ECANCELED, ENOTEMPTY, ETIMEDOUT, EBUSY,
    EWOULDBLOCK/EAGAIN, EADDRINUSE, EHOSTUNREACH, ENETUNREACH,
    ECONNREFUSED, EADDRNOTAVAIL, ECONNRESET, ENOTCONN, EMSGSIZE,
    ENOTSOCK, EPROTONOSUPPORT, EDESTADDRREQ, ENOMSG/ENODATA/EBADMSG,
    EMLINK, etc.). Unknown errnos return `Failed`.
  - 8 unit tests covering enum values, CONNECTION_CLOSED alias,
    to_code, quark, from_file_error mappings (all branches),
    from_errno via file_error, from_errno additional codes, unknown
    errno fallback — all passing.

- **`gnotification`** — GIO desktop notification. Mirrors
  `gio/gnotification.h` / `gio/gnotification.c`:
  - Upstream `GNotification` is a `GObject` subclass. We port it as
    a plain `pub struct` with the same fields and API, since the
    GObject interface system that `GIcon` (used by `set_icon`)
    depends on is deferred (Phase 9).
  - `NotificationPriority` enum (Normal=0 / Low=1 / High=2 / Urgent=3)
    matching upstream `GNotificationPriority` (note the unusual
    order — Normal is 0). `Default` impl returns `Normal`.
  - `NotificationButton` struct (label, action_name, target:
    Option<Variant>) mirroring the private `Button` struct in
    `gnotification.c`.
  - `NotificationIcon` opaque type alias
    (`Arc<dyn Any + Send + Sync>`) for deferred `GIcon` support.
    Callers can stash any icon-like value; real `GIcon` integration
    lands when the GObject interface system is ported.
  - `Notification` struct with title, body, icon, priority, category,
    buttons (Vec<NotificationButton>), default_action,
    default_action_target (Option<Variant>). Full setter API:
    `set_title`, `set_body`, `set_priority`, `set_urgent` (deprecated
    wrapper mapping true→Urgent / false→Normal), `set_category`,
    `add_button`, `add_button_with_target_value`,
    `set_default_action`, `set_default_action_with_target_value`,
    `set_icon`. Accessor methods for all fields (for testing and
    the kernel smoke check).
  - 16 unit tests covering priority values/default, construction
    defaults, all setters, button with/without target, default action
    with target + overwrite-clears-target semantics, opaque icon
    storage + downcast, clone preservation, button slice access —
    all passing.

- **`gsrvtarget`** — GIO SRV record target. Mirrors
  `gio/gsrvtarget.h` / `gio/gsrvtarget.c`:
  - `SrvTarget` boxed struct (hostname, port, priority, weight)
    with `Clone`/`Debug`/`PartialEq`/`Eq`/`Hash`. Upstream is a
    `G_DEFINE_BOXED_TYPE` with manual malloc/free; we use a plain
    `pub struct` (Rust's ownership handles copy/free).
  - `SrvTarget::new` / `hostname` / `port` / `priority` / `weight`
    matching `g_srv_target_new` / `_get_hostname` / `_get_port` /
    `_get_priority` / `_get_weight`.
  - `srv_target_list_sort(targets: Vec<SrvTarget>) -> Vec<SrvTarget>`
    implementing RFC 2782 priority+weight sorting:
    1. Single target with hostname `"."` → empty (service not
       available, per RFC 2782).
    2. Sort by (priority, weight) ascending — weight-0 targets come
       first within each priority group (matches upstream
       `compare_target`).
    3. For each priority group, repeatedly pick a target at random
       with probability proportional to weight (using
       `random_int_range`), remove it, append to output. When all
       weights are 0, pick deterministically.
    Upstream mutates a `GList` in place; we take ownership of the
    input `Vec` and return a new sorted `Vec` (idiomatic Rust
    equivalent).
  - 12 unit tests covering construction/accessors, clone, eq/hash,
    empty list sort, single-"."-hostname special case, single normal
    target, priority ordering, weight-0 targets present in group,
    all-targets-preserved, single priority-0 target, weighted
    distribution preserves all 10 targets, input consumption — all
    passing.

- **`ginetaddress`** — GIO IP address. Mirrors
  `gio/ginetaddress.h` / `gio/ginetaddress.c`:
  - `SocketFamily` enum (Invalid=0 / Unix=1 / Ipv4=2 / Ipv6=10) with
    `#[repr(i32)]` — discriminant values match Linux `AF_*` constants
    so they align with what a real OS uses.
  - `InetAddrBytes` enum (`Ipv4([u8; 4])` / `Ipv6([u8; 16])`) for the
    raw address bytes.
  - `InetAddress` plain-struct port of the upstream GObject subclass.
    Holds the address family + raw bytes. Upstream uses
    `G_DEFINE_TYPE_WITH_CODE` + `G_ADD_PRIVATE`; we use a plain struct
    with `Clone`/`Debug`/`PartialEq`/`Eq`/`Hash` (Rust's ownership
    handles ref/unref).
  - `new_from_string` — hand-written parser (no `inet_pton` /
    `getaddrinfo` in `no_std`). IPv4 dotted-quad with strict validation
    (no leading zeros, 0–255 per octet). IPv6 text form with `::`
    compression, embedded IPv4 (`::ffff:192.168.1.1`), and standard
    8-group full form. Returns `None` on malformed input.
  - `new_from_bytes` / `new_loopback` / `new_any` matching
    `g_inet_address_new_from_bytes` / `_new_loopback` / `_new_any`.
    IPv4 loopback = `127.0.0.1`, IPv6 loopback = `::1`; IPv4 any =
    `0.0.0.0`, IPv6 any = `::`. Invalid family / wrong byte count
    returns `None`.
  - `equal` / `to_string` / `to_bytes` / `native_size` / `family`
    matching the upstream accessors. `to_string` for IPv6 applies
    RFC 5952 `::` compression (longest run of zero groups ≥ 2, ties
    go to the first run).
  - Full classification suite: `is_any` (all-zero bytes), `is_loopback`
    (IPv4 `127.0.0.0/8`, IPv6 `::1`), `is_link_local` (IPv4
    `169.254.0.0/16`, IPv6 `fe80::/10`), `is_site_local` (IPv4
    `10.0.0.0/8` + `172.16.0.0/12` + `192.168.0.0/16`, IPv6
    `fec0::/10`), `is_multicast` (IPv4 `224.0.0.0/4`, IPv6
    `ff00::/8`), `is_mc_global` / `is_mc_link_local` /
    `is_mc_node_local` / `is_mc_org_local` / `is_mc_site_local` for
    multicast scope classification.
  - 18 unit tests covering socket family values, IPv4 parse/roundtrip/
    bytes/loopback/any/invalid (leading zeros, out-of-range, too many
    octets, non-numeric)/classification (loopback/link-local/site-
    local/multicast/scopes), IPv6 full form/compressed/loopback/any/
    new_loopback/new_any/embedded IPv4/classification/invalid (double
    `::`, non-hex, group too long), equal, format compression picks
    longest run, invalid family, wrong byte count, clone — all
    passing.

- **`fileutils` (addition)** — added `file_error_from_errno(err_no)`
  matching upstream `g_file_error_from_errno`. Maps 25 well-known
  errno values (EEXIST, EISDIR, EACCES, ENAMETOOLONG, ENOENT, ENOTDIR,
  ENXIO, ENODEV, EROFS, ETXTBSY, EFAULT, ELOOP, ENOSPC, ENOMEM,
  EMFILE, ENFILE, EBADF, EINVAL, EPIPE, EAGAIN, EINTR, EIO, EPERM,
  ENOSYS) to `FileError` variants; unknown errnos return `Failed`.
  Needed by `gioerror::io_error_from_errno`.

### Deferred

- Remaining GIO submodules: GInputStream / GOutputStream, GFile,
  GFileInfo, GCancellable, GAsyncResult, GIcon and implementations
  (GThemedIcon, GEmblem, GEmblemedIcon, GBytesIcon), GDataInput /
  GDataOutput streams, GBuffered streams, GMemory streams, sockets,
  D-Bus connection, GSettings, GApplication, GAction, GVolume, GMount,
  GDrive, GResolver, GNetworkAddress, GSocket, GDBus, etc.
- Most GIO types are GObject subclasses / interfaces, so this sub-
  phase requires the deferred GObject interface system (Phase 9
  deferred: GInterface vtable init + dispatch). The current ports
  pick leaf boxed types (`GFileAttributeInfoList`, the
  `GDBus*Info` structs, the `GDBusError` registry, and `GIOErrorEnum`
  are all `G_DEFINE_BOXED_TYPE` / `G_DEFINE_QUARK` / pure data) that
  don't depend on the interface system.
- `g_dbus_node_info_new_for_xml` (XML parsing of introspection data)
  and `g_dbus_interface_info_generate_xml` / `_node_info_generate_xml`
  (XML generation) — deferred; need GMarkup parser integration and
  the introspection parser state machine.
- `g_dbus_interface_info_cache_build` / `_release` (per-interface
  name→info lookup cache) — deferred; needs a global cache table.
- `g_dbus_error_set_dbus_error` / `_valist` (printf-style error
  construction) — deferred; need printf-style formatting which is
  already partially ported in `printf.rs` but not wired here.
- `g_io_error_from_win32_error` (Windows error → IOErrorEnum) —
  deferred; not applicable to bare-metal RustOS.

## Phase 10 detail (partial)

### Modules

- **`gmodule`** — `GModule` ref-counted handle mirroring `struct _GModule`
  in `gmodule.c` (file_name, platform handle, ref_count, is_resident,
  unload callback, next pointer). The `ModulePlatform` trait supplies the
  OS-specific dynamic loader primitives (`open`, `self_handle`, `symbol`,
  `close`, `build_path`, `supported`), mirroring the static `_g_module_*`
  helpers in `gmodule.c`. `NoModulePlatform` is a no-op implementation for
  bare-metal kernels (RustOS) so the GLib API surface is linkable even
  when no dynamic linker is available — every operation returns the
  upstream "dynamic modules are not supported by this system" error
  string and `module_supported` returns `false`. The cross-platform
  registry logic (global `MODULES` list + `MAIN_MODULE` singleton,
  name/handle dedup, ref-count bump on re-open, `g_module_check_init` /
  `g_module_unload` symbol lookup after open, resident marking) lives in
  Rust and is platform-agnostic. Public API: `module_supported`,
  `module_open`, `module_open_full`, `module_close`, `module_symbol`,
  `module_make_resident`, `module_error`, `module_name`,
  `module_build_path`, `module_error_quark`, plus the `GModule`,
  `GModuleFlags`, `GModuleError`, `GModuleCheckInit`, `GModuleUnload`,
  `ModuleHandle`, `ModulePlatform`, `NoModulePlatform` types. 20 unit
  tests covering flags/error codes/quark/build_path/registry/error
  paths/resident marking/ref_count/name resolution — all passing on the
  host test target.

### Deferred

- Real `dlopen`/`dlsym`/`dlclose` platform implementation (needs OS
  dynamic linker support — not applicable to bare-metal RustOS).
- `parse_libtool_archive` (libtool `.la` file parser) — deferred until a
  real platform implementation lands; the `.la` parsing path is
  unreachable on `NoModulePlatform`.
- `GModuleDebugFlags` (`resident-modules`, `bind-now-modules`) env-var
  parsing — deferred until `g_getenv` is wired to a real environment.
- Per-thread error storage (`GPrivate`/thread-local in upstream) —
  currently a single global `Mutex<Option<String>>`, which matches
  upstream behaviour in the single-threaded kernel boot environment.

## Phase 9 detail (partial)

### Modules

- **`gtype`** — GType ID system with 21 fundamental types (matching GLib constants),
  type registry backed by `spin::RwLock<BTreeMap>`, `type_register_fundamental`,
  `type_register_static`, `type_register_static_simple`, `type_from_name`, `type_name`,
  `type_parent`, `type_fundamental`, `type_is_a` (hierarchy walk), `type_depth`,
  `type_children`, `type_interfaces`, `type_query`, `type_add_interface`,
  `type_is_classed`/`type_is_instantiatable`/`type_is_abstract`/`type_is_final`.
  `GTypeFundamentalFlags`, `GTypeFlags`, `GTypeInfo`, `GTypeValueTable`.
- **`gvalue`** — `GValue` polymorphic container with typed getters/setters for all
  basic types (bool, int, uint, int64, uint64, float, double, char, uchar, long,
  ulong, string, pointer, enum, flags, object, boxed). `value_new_*` helpers.
  `copy_from`, `reset`, `clear`, `holds`, `default_value_table_for`.
- **`gparamspec`** — `ParamSpec` with typed constructors for bool, int, uint, int64,
  uint64, float, double, char, uchar, long, ulong, string, enum, flags, object,
  pointer. `ParamFlags` (readable, writable, construct, construct-only, etc.).
  `install_properties`, `find_property`, `find_property_by_id`, `property_names`.
- **`gsignal`** — Signal registry with `signal_new`, `signal_lookup`, `signal_query`,
  `signal_name`, `signal_list_ids`. Handler connection via `signal_connect`/
  `signal_connect_by_name`, `signal_handler_disconnect`, `signal_handler_is_connected`.
  Signal emission with `signal_emit`/`signal_emit_by_name` supporting `RUN_FIRST`/
  `RUN_LAST`/`AFTER` ordering. `SignalFlags`, `ConnectFlags`, `SignalCallback`
  (Arc<dyn Fn>). `signal_handlers_disconnect_all`, `signal_n_handlers`.
- **`gobject`** — `GObject` base class with atomic ref counting (`ref_`/`unref`/
  `ref_sink`/`is_floating`), property system (`install_properties`, `get_property`,
  `set_property` with validation + notify signal), `list_properties`, user data
  (`set_data`/`get_data`/`remove_data`), weak references (`add_weak_ref`/`clear_weak_refs`),
  signal helpers (`connect_signal`/`emit_signal`), `PropertyBinding` for sync.
  `object_new`/`object_new_with_params` convenience functions.

### Deferred

- GInterface vtable initialization and dispatch.
- GObject closure system (`GClosure`).
- GParamSpec pool and override.
- GValue transform functions between types.
- GType plugin system (dynamic type registration).
- C ABI compatibility (`extern "C"` wrappers, `GTypeInstance` layout).

## Phase 7 detail (partial)

### Modules

- **`date`** — `Date` with pure math (day/month/year arithmetic, weekday calculation).
- **`datetime`** — `DateTime` with UTC arithmetic, formatting, parsing.
- **`timezone`** — `TimeZone` with fixed offsets (no IANA database).
- **`varianttype`** — `VariantType` parser and validator.
- **`variant`** — `Variant` value container, `VariantBuilder`, parser.
- **`unicode`** — Types, enums, combining class, basic `unichar` functions.
- **`utf8`** — UTF-8 encoding/decoding, `utf8_get_char`, `utf8_next_char`.

### Deferred

- IANA timezone database integration.
- Full Unicode decomposition/case tables.
- `g_unichar_totitle`, full break properties.

## Phase 6 detail (partial)

### Modules — fully implemented (pure logic)

- **`fileutils`** — `FileError`, `FileTest`, path utilities.
- **`convert`** — `ConvertError`, URI helpers, charset conversion types.
- **`charset`** — Charset name normalization stubs.
- **`checksum`** — `Checksum`, `ChecksumType` (MD5, SHA1, SHA256, SHA512).
- **`base64`** — `Base64Encoder`, `Base64Decoder`.
- **`hmac`** — `Hmac` using checksum internals.
- **`hostutils`** — Hostname validation/encoding.
- **`environ`** — `getenv`/`setenv`/`unsetenv` with global `BTreeMap`.
- **`keyfile`** — `KeyFile` parser/writer.
- **`bitlock`** — Bit-level locking primitives.
- **`hook`** — `HookList` with `Hook` nodes.
- **`pattern`** — `PatternSpec` glob matching.
- **`shell`** — `shell_quote`, `shell_unquote`, `shell_parse_argv`.
- **`uri`** — `Uri`, `UriFlags`, `UriHideFlags`, `UriError`, join/escape/unescape.
- **`markup`** — `MarkupParser`, `MarkupError`, `MarkupParseFlags`.
- **`stringchunk`** — `StringChunk` interning pool.
- **`strvbuilder`** — `StrvBuilder` string vector builder.
- **`version`** — Version constants and `check_version`.
- **`scanner`** — `GScanner` lexical scanner with configurable rules and symbol tables.
- **`timer`** — `Timer` stopwatch with injectable monotonic clock.
- **`utils`** — `get_prgname`/`set_prgname`, `get_application_name`/`set_application_name`,
  OS info keys, `USEC_PER_SEC`, `NSEC_PER_SEC`.
- **`pathbuf`** — `PathBuf` incremental path builder.
- **`uuid`** — `uuid_string_is_valid`, `uuid_string_random` (RFC 4122 v4).
- **`regex`** — **Real backtracking regex engine**: literals, `.`, `*`, `+`, `?`,
  `{n,m}` quantifiers, character classes `[abc]`/`[^abc]`/`[a-z]`, anchors `^`/`$`,
  word boundaries `\b`/`\B`, alternation `|`, capturing/non-capturing groups,
  escapes `\d`/`\D`/`\w`/`\W`/`\s`/`\S`, case-insensitive flag, greedy/lazy
  quantifiers, `split()`, `replace()` with `$1` capture substitution.
  35 unit tests. NOT a stub — actual pattern compilation and matching.
- **`testutils`** — `TestCase`, `TestSuite`, assertion helpers (`assert_true`,
  `assert_cmpstr`, etc.), `TestTrapFlags`, `test_trap_subprocess`.

### Modules — types/flags/errors ported (OS ops via platform traits)

- **`dir`** — `Dir` iterator, `DirError`, `DirPlatform` trait. `NoDirPlatform` stub.
  Real directory reading needs OS `getdents`/`readdir`.
- **`mappedfile`** — `MappedFile` with `Vec<u8>` contents, `MappedFileError`,
  `MappedFilePlatform` trait. `NoMappedFilePlatform` stub.
  Real memory mapping needs OS `mmap`.
- **`spawn`** — `SpawnError` (20 error codes with errno mapping), `SpawnFlags`,
  `SpawnResult`, `Pid`, `SpawnPlatform` trait. `NoSpawnPlatform` stub.
  Real process spawning needs OS `fork`/`exec`.
- **`stdio`** — `StatBuf` (with `is_file`/`is_dir`/`is_symlink`/`is_executable`),
  `OpenFlags`, file mode constants, `StdioPlatform` trait. `NoStdioPlatform` stub.
  Real file I/O needs OS syscalls.

### Deferred

- PCRE2-compatible regex features (lookahead/lookbehind, named groups, backreferences).
- Actual OS operations for dir/spawn/stdio/mappedfile (requires platform implementations).

## Phase 5 detail

### Modules

- **`error`** — `Error { domain, code, message }`, propagation (`propagate_error`),
  prefixing, overwrite warnings via the logging layer.
- **`messages`** — `LogLevelFlags`, `g_log` handler routing, default handler formatting,
  `g_print` / `g_printerr` hooks.
- **`option`** — `OptionContext`, `OptionGroup`, `OptionEntry`, argv parsing for bool,
  string, int, string-array, filename, and callback arg types.

### Deferred

- Extended error domains (`G_DEFINE_EXTENDED_ERROR`, `g_error_domain_register*`).
- Structured logging (`g_log_structured*`, GVariant fields).
- Full goption arg types (`Double`, `Int64`, help text generation).

## Phase 4 detail

### Modules

- **`quark`** — string interning and quark table.
- **`dataset`** — per-object `DataList` key/value attachments.
- **`hash`** — open-addressing `HashTable` with prime moduli and tombstones.
- **`tree`** — AVL `Tree` with traverse, search, and destroy callbacks.
- **`relation`** (deprecated) — `Relation`/`Tuples` with `BTreeMap` indexing.
- **`cache`** (deprecated) — `Cache` key-value cache with reference counting.

## Phase 3 detail

### Modules

- **`array`** — `GArray`, `ByteArray` with `Mutex`-protected state and atomic ref counting.
- **`list`** — safe `List` / `SList` wrappers; `GList` / `GSList` remain `#[repr(C)]` layouts.
- **`queue`** — generic `GQueue<T>` double-ended queue.
- **`ptr_array`** — `PtrArray` pointer array with ref counting and free-func support.
- **`node`** — `Node`/`NTree` n-ary tree with traverse flags.
- **`sequence`** — `Sequence`/`SequenceIter` sorted sequence.
- **`completion`** (deprecated) — `Completion` with prefix search.
- **`qsort`** — `sort_array`, `sort_array_unstable`.
- **`primes`** — `spaced_primes_closest`.

## Phase 2 detail

### Modules

- **`atomic`** — `AtomicInt`, `AtomicUInt`, `AtomicPointer` matching `g_atomic_*`.
- **`mem`** — `malloc`/`realloc`/`memdup` family, aligned alloc, `clear`/`steal`.
- **`strfuncs`** — non-printf string helpers (`strdup`, `strjoin`, ASCII compare, strip).
- **`gstring`** — growable `GString` buffer with GLib length/nul semantics.
- **`rand`** — `Rand` Mersenne Twister MT19937 RNG with global functions.
- **`printf`** — `sprintf`, `vsprintf`, `printf_format` wrappers.
- **`slice`** (deprecated) — `slice_alloc`/`slice_free1` delegating to `alloc::alloc`.

## Phase 1 detail

### Modules

- **`endian`** — `GUINT16/32/64_SWAP_LE_BE`, host/network byte order helpers
  from `gtypes.h`.
- **`checked`** — `g_*_checked_add` / `g_*_checked_mul` overflow-safe arithmetic.
- **`refcount`** — `grefcount` (single-threaded) and `gatomicrefcount` semantics.
- **`bytes`** — Immutable reference-counted byte buffer matching `GBytes` behavior
  (inline storage for small buffers, slicing, hash, compare).
- **`refstring`** — `RefString` wrapping `Arc<str>` for reference-counted strings.
- **`rcbox`** — `RcBox`/`AtomicRcBox` wrapping `Arc<T>` for reference-counted boxes.

## Running total

**59 modules** ported across Phases 1–11, all wired into RustOS:

| Phase | Modules | Status |
|-------|---------|--------|
| 1 | endian, checked, refcount, bytes, refstring, rcbox | Done (6) |
| 2 | atomic, mem, strfuncs, gstring, rand, printf, slice | Done (7) |
| 3 | array, list, queue, ptr_array, node, sequence, completion, qsort, primes | Done (9) |
| 4 | hash, tree, dataset, quark, relation, cache | Done (6) |
| 5 | error, messages, option | Done (3) |
| 6 | fileutils (+ file_error_from_errno), convert, charset, checksum, base64, hmac, hostutils, environ, keyfile, bitlock, hook, pattern, shell, uri, markup, stringchunk, strvbuilder, version, scanner, timer, utils, pathbuf, uuid, regex, testutils + dir/mappedfile/spawn/stdio (stubs) | Partial (29) |
| 7 | date, datetime, timezone, varianttype, variant, unicode, utf8 | Partial (7) |
| 8 | asyncqueue, thread, poll, iochannel, mainloop, threadpool | Partial (6) |
| 9 | gtype, gvalue, gparamspec, gsignal, gobject | Partial (5) |
| 10 | gmodule | Partial (1) |
| 11 | gfileattribute, gdbusintrospection, gdbuserror, gioerror, gnotification, gsrvtarget, ginetaddress | Partial (7) |

### RustOS smoke test coverage

The `smoke_check()` function in `rust-os/src/glib.rs` validates at boot:
- Checked arithmetic, byte swaps, base64 encode/decode
- `GBytes`, `GChecksum`, `GHmac`, `GCharset` defaults
- Path helpers, filename-to-URI roundtrip
- Quark interning, refcount, string predicates
- `GString`, `GByteArray`, pattern matching, shell quote/parse
- Variant type validation, `GVariant` tuple
- `GQueue`, `GArray`, `GPtrArray`, `GHashTable`, `GKeyFile`
- `GUri` parse/build/join, GLib version checks
- `GMarkup` parse, `GStringChunk`, `GNode` tree
- `GError`, `GOption` parse, `GAsyncQueue`, `GSequence`
- `GHook` list, `GAtomicInt`/`GAtomicUInt`, `GDataList`
- `GDateTime`, `GTimer`, `GTimeZone`, `GScanner`, `GChecksum` digest
- `GBitLock`, `GPointerBitLock`
- **Regex** — compile, match with capture groups, `match_simple`, `split`, `replace`
- **ThreadPool** — create, push, unprocessed count
- **TestSuite** — create, add cases, count
- **GType** — fundamental type lookup, static type registration, hierarchy check
- **GValue** — int and string value create/get
- **GSignal** — register, lookup, query metadata
- **GObject** — create, ref/unref, property install/set/get
- **GModule** — `module_supported` / `module_error_quark` /
  `module_build_path` (Linux-style `libNAME.so` decoration) /
  `module_open_full` / `module_open` (None = main program) /
  `module_symbol` / `module_close` all return the upstream
  "not supported" error on `NoModulePlatform`; `GModule::new` +
  `name`/`ref_count`/`is_resident`/`make_resident` struct behaviour
  exercised directly
- **GFileAttributeInfoList** — `FileAttributeType` enum values,
  `FileAttributeInfoFlags` BitOr/contains, empty list, sorted insert
  (3 attrs inserted in non-sorted order stay sorted by name),
  `lookup` returns the right type+flags, re-adding an existing name
  updates in place, `dup` produces an independent copy, `ref_` bumps
  the ref count (and drop decrements)
- **GDBusIntrospection** — `DBusPropertyInfoFlags` BitOr/contains,
  `dbus_annotation_info_lookup` (hit + miss), full D-Bus interface
  hierarchy construction (node → interface → method/signal/property
  with args + annotations), `dbus_node_info_lookup_interface` /
  `dbus_interface_info_lookup_method` / `_signal` / `_property`
  (hit + miss), method in/out args preserved, property flags
  preserved, `ref_` returns the same Arc handle, dropping the node
  Arc leaves the interface Arc alive
- **GDBusError** — `DBusError` enum values + `to_dbus_name`,
  `dbus_error_quark` (non-zero + stable across calls), well-known
  entry registration (Failed code 0 → org.freedesktop.DBus.Error.Failed),
  `dbus_error_is_remote_error` / `_get_remote_error` (registered +
  prefix-parse fallback), `dbus_error_strip_remote_error` (strips +
  returns false on local), `dbus_error_new_for_dbus_error` (registered
  domain + message prefix), `dbus_error_encode_gerror` (registered
  name + unmapped form with `_2d` hex escapes for hyphens + `.Code<N>`
  suffix), unmapped-form round-trip (encode → new_for_dbus_error
  recovers domain + code), custom register/unregister semantics,
  `parse_remote_prefix` with colons in the message body
- **GIOErrorEnum** — `IOErrorEnum` enum values (Failed=0, NotFound=1,
  BrokenPipe=44, NoSuchDevice=47, DestinationUnset=48),
  `CONNECTION_CLOSED` const alias for `BrokenPipe`, `io_error_quark`
  non-zero, `io_error_from_file_error` (Exist→Exists, NoEnt→NotFound,
  Acces→PermissionDenied, NoSpc→NoSpace, Pipe→BrokenPipe,
  Failed→Failed), `io_error_from_errno` (ENOENT→NotFound,
  EEXIST→Exists, EACCES→PermissionDenied, ENOSPC→NoSpace,
  EINVAL→InvalidArgument, ECANCELED→Cancelled, ETIMEDOUT→TimedOut,
  EADDRINUSE→AddressInUse, ECONNREFUSED→ConnectionRefused,
  ECONNRESET→CONNECTION_CLOSED, ENOTCONN→NotConnected, unknown→
  Failed), `file_error_from_errno` (ENOENT→NoEnt, EEXIST→Exist,
  unknown→Failed)
- **GNotification** — `NotificationPriority` values (Normal=0, Low=1,
  High=2, Urgent=3), `Notification::new` defaults (empty body,
  Normal priority, no buttons, no default action, no icon), all
  setters (title/body/priority/category), `set_urgent` mapping
  (true→Urgent, false→Normal), buttons with and without `Variant`
  targets (label + action_name + target preserved), default action
  with target + overwrite-clears-target semantics, opaque icon
  storage + `downcast_ref` retrieval
- **GSrvTarget** — `SrvTarget::new` + accessors (hostname/port/
  priority/weight), `srv_target_list_sort` empty list → empty,
  single `"."` hostname → empty (RFC 2782 not-available), sort by
  priority ascending (10/20/30), all targets survive weighted-random
  selection within a priority group
- **GInetAddress** — `SocketFamily` values (Invalid=0, Ipv4=2,
  Ipv6=10), IPv4 parse + roundtrip (`192.168.1.1`), IPv4 loopback
  (`127.0.0.1`) + any (`0.0.0.0`) + classification (site-local
  `10/172.16/192.168`, link-local `169.254`, multicast `224.x` +
  scopes), IPv4 invalid rejection (too few octets, out-of-range,
  non-numeric), IPv6 parse + compression (`2001:db8::1`), IPv6
  loopback (`::1`) + any (`::`), IPv6 embedded IPv4
  (`::ffff:192.168.1.1`), IPv6 link-local (`fe80::`) + multicast
  scopes (`ff02`/`ff0e`), `equal`, `new_from_bytes` byte-count
  validation

## Running Rust tests

```bash
cd rust && cargo test
```

## Contributing

When adding a new converted module:

1. Add the module under `rust/glib-native/src/`.
2. Port or mirror relevant tests from `glib/tests/`.
3. Update the phase table in this document.
4. Do not remove C sources until the phase’s FFI integration is complete and
   upstream tests pass with the Rust backend.
