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
| **6** | I/O primitives | `gfileutils.*`, `gconvert.*`, … | **Done** (pure logic + `DirPlatform`/`MappedFilePlatform`/`StdioPlatform`/`SpawnPlatform`; RustOS VFS wired in `src/glib_platform.rs`) |
| **7** | Date/time & variants | `gdate.*`, `gdatetime.*`, `gtimezone.*`, … | **Done** (TZif v1/v2, embedded IANA, `unicode_norm` NFD/NFC for Latin-1 + Extended-A) |
| **8** | Main loop & threading | `gmain.*`, `gthread.*`, … | **Done** (`PollPlatform`/`g_poll`, `HostPollPlatform` with real `poll(2)` on host tests, kernel timer poll) |
| **9** | GObject core | `gobject/*` | **Done** (Rust GObject stack complete; `ffi` module exports ~70 `g_*` symbols for C interop) |
| **10** | GModule | `gmodule/*` | **Done** (`NoModulePlatform` kernel default; `HostModulePlatform` + `parse_libtool_archive` on host tests) |
| **11** | GIO | `gio/*` | **Done** (~230 modules; real zlib via `miniz_oxide`, `io_error_from_win32_error`, loopback D-Bus, platform stubs) |
| **12** | GObject Introspection & tools | `girepository/*`, `tools/*` | **Done** (full `gi*` module set, binary `Typelib::from_bytes`, `Repository::require` loads from search paths; CLI tools complete) |
| **13** | Remove C implementations; stable C ABI | all | **Done** (`ffi` + `ffi_parity`, `libglib_native.a` staticlib, `include/glib_native.h`, C smoke test; upstream C sources retained for diff/parity until Meson `rust-native` switch) |

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

All 448 ported modules are declared in `glib-native/src/lib.rs`. Kernel integration
re-exports selected types in `src/glib.rs` via `pub use glib_native::*` plus an explicit
alphabetical re-export list for documentation and name resolution.

## Phase 8 detail (partial)

### Modules

- **`asyncqueue`** — `AsyncQueue<T>` with `GMutex`/`GCond` for blocking pop.
- **`thread`** — `GMutex<T>`, `GRecMutex`, `GRWLock`, `GCond`, `Once` wrapping `spin` primitives.
- **`poll`** — `PollFD`, `IOCondition`, `PollFunc` types.
- **`iochannel`** — I/O channel types and enums.
- **`mainloop`** — `MainContext`, `MainLoop`, `Source` with `prepare`/`check`/`dispatch` callbacks,
  `timeout_add`, `idle_add`, `source_remove`, `pending()`, `iteration(may_block)` (dispatches
  ready timeout/idle sources using monotonic clock deadlines).
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

- **`ginetaddressmask`** — GIO IP address mask (subnet). Mirrors
  `gio/ginetaddressmask.h` / `gio/ginetaddressmask.c`:
  - `InetAddressMaskError` enum (NoAddress / LengthTooLong /
    BitsBeyondPrefix / ParseFailed) matching the upstream
    `G_IO_ERROR_INVALID_ARGUMENT` cases.
  - `InetAddressMask` plain-struct port of the upstream
    `GObject` + `GInitable` subclass. Holds a base `InetAddress` and
    a prefix length (0–32 for IPv4, 0–128 for IPv6). Upstream uses
    `g_initable_new` + a `GInitableIface` to validate at construction
    time; we use `Result`-returning constructors.
  - `InetAddressMask::new(addr, length)` — validates that
    `length <= addr.native_size() * 8` (LengthTooLong) and that all
    bits in `addr` beyond position `length` are 0 (BitsBeyondPrefix),
    including the partial-byte case (e.g. `/25` masks require the low
    7 bits of the 4th byte to be 0).
  - `InetAddressMask::new_from_string(s)` — parses `"addr/length"` or
    just `"addr"` (in which case the length is the full address size
    in bits). Returns `ParseFailed` on malformed input (bad address,
    empty length, non-numeric length) and propagates `new`'s
    validation errors.
  - `to_string` — returns `"addr/length"`, or just `"addr"` if
    `length` is the full address size (matching upstream).
  - `family` / `address` / `length` accessors.
  - `matches(address)` — returns `false` if families differ, `true`
    if `length == 0` (matches everything), otherwise compares the
    first `length` bits with full-byte `memcmp` + partial-byte
    masking (`addr_byte & (0xff << (8 - nbits)) == mask_byte`).
  - `equal(other)` — same length + same base address.
  - 22 unit tests covering full-length/zero-length/too-long
    construction, bits-beyond-prefix (full byte + partial byte),
    IPv4/IPv6 from_string with/without length, invalid strings
    (non-ip, empty length, non-numeric length, bits beyond prefix,
    length too long), to_string omits full length, matches (within/
    outside/partial-byte/zero-length/full-length/different-family/
    IPv6), equal (same/different-length/different-address), clone —
    all passing.

- **`gnetworkaddress`** — GIO network address. Mirrors
  `gio/gnetworkaddress.h` / `gio/gnetworkaddress.c`:
  - `NetworkAddressError` enum (UnclosedBracket / BadBracket /
    EmptyPort / InvalidPort / UnknownService / InvalidUri) matching
    the upstream `G_IO_ERROR_INVALID_ARGUMENT` cases.
  - `NetworkAddress` plain-struct port of the upstream `GObject` +
    `GSocketConnectable` subclass. Holds hostname, port, and optional
    scheme (set by `parse_uri`). Upstream also caches resolved
    `GSocketAddress`s; we skip that (needs DNS resolution + the
    `GSocketConnectable` interface).
  - `new(hostname, port)` / `new_loopback(port)` matching
    `g_network_address_new` / `_new_loopback` (loopback uses
    `"localhost"`).
  - `parse(host_and_port, default_port)` — the full upstream parsing
    algorithm: bracketed IPv6 (`[addr]` or `[addr]:port`), `host:port`
    with numeric port, unescaped IPv6 (multiple `:` → no port), plain
    hostname (uses `default_port`). Service names (e.g. `"http"`)
    return `UnknownService` since `no_std` has no `getservbyname`.
  - `parse_uri(uri, default_port)` — uses the ported `Uri::parse` to
    extract scheme/host/port, then applies `default_port` if the URI
    has no port.
  - `hostname` / `port` / `scheme` accessors + `equal`.
  - 20 unit tests covering `new`/`new_loopback`/`parse` (plain
    hostname, host:port, bracketed IPv6 with/without port, unescaped
    IPv6, empty port, invalid numeric port, service name, unclosed
    bracket, bad bracket, max port, port zero)/`parse_uri` (with
    port, no port → default, invalid)/`equal` (same/different-port/
    different-hostname/with-scheme-vs-without)/`clone` — all passing.

- **`fileutils` (addition)** — added `file_error_from_errno(err_no)`
  matching upstream `g_file_error_from_errno`. Maps 25 well-known
  errno values (EEXIST, EISDIR, EACCES, ENAMETOOLONG, ENOENT, ENOTDIR,
  ENXIO, ENODEV, EROFS, ETXTBSY, EFAULT, ELOOP, ENOSPC, ENOMEM,
  EMFILE, ENFILE, EBADF, EINVAL, EPIPE, EAGAIN, EINTR, EIO, EPERM,
  ENOSYS) to `FileError` variants; unknown errnos return `Failed`.
  Needed by `gioerror::io_error_from_errno`.

- **`gcancellable`** — GIO cancellation primitive. Mirrors
  `gio/gcancellable.h` / `gio/gcancellable.c`:
  - `GCancellable` plain-struct port of upstream GObject subclass.
    Holds thread-safe cancellation state protected by a `spin::Mutex`.
  - `new` / `is_cancelled` / `set_error_if_cancelled` (propagates
    `Cancelled` IOErrorEnum) / `reset` / `cancel` (triggers all
    connected callbacks).
  - Callback connection API: `connect` (inline execute if already
    cancelled, else stores callback and returns unique u32 ID) and
    `disconnect` (blocks safely if callbacks are currently running).
  - Thread-safe global stack of current cancellables: `get_current`,
    `push_current`, `pop_current` (backed by a global Mutex-protected stack).
  - Stubbed `get_fd` (returns `-1`), `make_pollfd` (returns `false`), and `release_fd`
    for `no_std` environments.
  - `cancellable_source_new` helper returning a `GSource` (MainLoop `Source`)
    ready if the cancellable is cancelled.
  - 37 unit tests covering creation, cancel state, error propagation, reset,
    immediate vs deferred callback connection, thread-safe disconnect, stack
    operations, and polling stubs — all passing.

- **`ginputstream`** — GIO input streams. Mirrors `gio/ginputstream.h` / `gio/ginputstream.c` / `gio/gmemoryinputstream.c`:
  - `InputStream` wrapper holding a trait object `Arc<dyn InputStreamImpl + Send + Sync>`.
  - `InputStreamImpl` trait with `as_any` for type downcasting.
  - `read`, `read_all` (returns read count + optional error), `skip`, `close`, `is_closed`, `has_pending`, `set_pending`, `clear_pending` with GCancellable checks.
  - `MemoryInputStream` concrete implementation backed by `spin::Mutex` guarding `Bytes` cursor state, support for `add_bytes` (appends data dynamically).
  - 7 unit tests covering read, read_all, skip, dynamic data appends, close, pending, and cancellation behavior.

- **`goutputstream`** — GIO output streams. Mirrors `gio/goutputstream.h` / `gio/goutputstream.c` / `gio/gmemoryoutputstream.c`:
  - `OutputStream` wrapper holding `Arc<dyn OutputStreamImpl + Send + Sync>`.
  - `OutputStreamImpl` trait with `as_any` for type downcasting.
  - `write`, `write_all`, `splice` (copies data from InputStream to OutputStream with flag-controlled child closing and cancellation), `flush`, `close`, `is_closed`, `has_pending`, `set_pending`, `clear_pending`.
  - `OutputStreamSpliceFlags` (None / CloseSource / CloseTarget).
  - `MemoryOutputStream` dynamically resizable or fixed-size in-memory buffer with `steal_as_bytes` and `get_data`/`get_data_size` accessors.
  - 8 unit tests covering resizing, data extraction, cancellation, and splicing.

- **`giostream`** — GIO bidirectional stream. Mirrors `gio/giostream.h` / `gio/giostream.c`:
  - `IOStream` struct wrapping an `InputStream` and `OutputStream` with `new`, `get_input_stream`, `get_output_stream`, `close` (closes child streams), `is_closed`, `has_pending`, `set_pending`, `clear_pending`.
  - 4 unit tests covering stream creation, child retrieval, close, pending, and cancellation.

- **`gfile`** — GIO file operations. Mirrors `gio/gfile.h` / `gio/gfile.c` / `gio/gfileinfo.h`:
  - `FileType` enum (Unknown / Regular / Directory / SymbolicLink / Special / ShortCut / Mountable).
  - `FileInfo` metadata container with setters/getters (size, type, name, generic attributes map).
  - `FileCreateFlags` (None / ReplaceDestination / Private) and `FileQueryInfoFlags` (None / NofollowSymlinks).
  - `File` handle (path + URI) with `new_for_path`, `new_for_uri`, `new_for_commandline_arg`, `get_path`, `get_uri`, `get_basename`, `get_parent`.
  - Backend integration: `FilePlatform` trait (read, create, replace, query_exists, query_info) with static `register_file_platform` hook, allowing bare-metal / mock VFS drivers.
  - `NoFilePlatform` default stub that returns `NotSupported`.
  - 4 unit tests covering path/URI parsing, mock platform registration, read/create/replace, and file info querying.

- **`gseekable`** — GIO seekable interface. Mirrors `gio/gseekable.h` / `gio/gseekable.c`:
  - `Seekable` trait (`can_seek` / `seek` / `can_truncate` / `truncate`) integrated into `InputStreamImpl` / `OutputStreamImpl` with default not-supported implementations.
  - Re-exports `SeekType` (Set / Cur / End) from `iochannel`.
  - `MemoryInputStream` implements seek (Set/Cur/End with bounds check); `MemoryOutputStream` implements seek + truncate.
  - 2 unit tests covering seek on memory streams and truncate.

- **`gdatainputstream`** — GIO typed-read stream. Mirrors `gio/gdatainputstream.h` / `gio/gdatainputstream.c`:
  - `DataStreamByteOrder` enum (BigEndian / LittleEndian / HostEndian) and `DataStreamNewlineType` enum (Lf / Cr / CrLf / Any).
  - `DataInputStream` wrapping `InputStream` with `Mutex`-protected byte order + newline settings.
  - `new` / `set_byte_order` / `get_byte_order` / `set_newline_type` / `get_newline_type` / `get_base_stream`.
  - Typed readers: `read_byte`, `read_int16`/`read_uint16`, `read_int32`/`read_uint32`, `read_int64`/`read_uint64` (respects byte order); `read_line` / `read_line_utf8` (handles all `DataStreamNewlineType` variants, returns `Option<String>`); `read_upto` (reads until stop-char set, seekable streams only).
  - 6 unit tests covering byte order, integer reads, newline types, and upto reads.

- **`gdataoutputstream`** — GIO typed-write stream. Mirrors `gio/gdataoutputstream.h` / `gio/gdataoutputstream.c`:
  - `DataOutputStream` wrapping `OutputStream` with `Mutex`-protected byte order.
  - `new` / `set_byte_order` / `get_byte_order` / `get_base_stream`.
  - Typed writers: `put_byte`, `put_int16`/`put_uint16`, `put_int32`/`put_uint32`, `put_int64`/`put_uint64`, `put_string`; `downcast_ref` for inner `MemoryOutputStream` access.
  - 2 unit tests covering big-endian and little-endian writes with data verification via downcast.

- **`gasyncresult`** — GIO async result interface. Mirrors `gio/gasyncresult.h`:
  - `AsyncResult` trait (`get_user_data` / `get_source_object` / `is_tagged`).
  - 1 unit test covering trait object construction.

- **`gpermission`** — GIO permission object. Mirrors `gio/gpermission.h` / `gio/gpermission.c`:
  - `Permission` struct with `Mutex`-protected state (allowed / can_acquire / can_release).
  - `new` / `get_allowed` / `get_can_acquire` / `get_can_release` / `impl_update` (atomically sets all three flags).
  - `acquire` (returns `NotSupported` when `!can_acquire`) / `release` (returns `NotSupported` when `!can_release`), both accept `Option<&GCancellable>`.
  - 7 unit tests covering default state, `impl_update`, acquire/release success and failure paths.

- **`gsimplepermission`** — GIO fixed-permission. Mirrors `gio/gsimplepermission.h` / `gio/gsimplepermission.c`:
  - `SimplePermission` wrapping `Permission` with a fixed `allowed` value; acquire and release always return `NotSupported`.
  - `new(allowed: bool)` / `get_allowed`.
  - 3 unit tests covering `new(true)`, `new(false)`, and immutability.

- **`gconverter`** — GIO converter interface. Mirrors `gio/gconverter.h`:
  - `ConverterFlags` enum (NoFlags / InputAtEnd / Flush) with `BitOr`.
  - `ConverterResult` enum (Error / Converted / Finished / Flushed).
  - `Converter` trait (`convert` / `reset`).
  - 5 unit tests covering flag values, result variants, and a trivial pass-through converter.

- **`gaction`** — GIO action interface. Mirrors `gio/gaction.h` / `gio/gaction.c`:
  - `Action` trait (`get_name` / `get_parameter_type` / `get_state_type` / `get_state_hint` / `get_enabled` / `get_state` / `change_state` / `activate`).
  - `action_name_is_valid` — validates action names (alphanumeric + `-` + `.`, no leading digit).
  - `action_parse_detailed_name` — parses `"name(target)"` into name + `Variant` target.
  - `action_print_detailed_name` — formats name + optional `Variant` back to `"name(target)"` or `"name"`.
  - 13 unit tests covering valid/invalid names, round-trip parse/print, and edge cases.

- **`gsimpleaction`** — GIO simple action. Mirrors `gio/gsimpleaction.h` / `gio/gsimpleaction.c`:
  - `SimpleAction` struct with `Mutex`-protected state (name / parameter_type / state_type / state / state_hint / enabled).
  - `new(name, parameter_type)` / `new_stateful(name, parameter_type, initial_state)`.
  - `set_enabled` / `set_state` / `set_state_hint`; implements `Action` trait.
  - 7 unit tests covering construction defaults, enable toggle, stateful action, and `change_state`.

- **`gfilterinputstream`** — GIO filter input stream. Mirrors `gio/gfilterinputstream.h` / `gio/gfilterinputstream.c`:
  - `FilterInputStream` wrapping `InputStream` with `Mutex`-protected `close_base_stream` flag (defaults `true`).
  - `new` / `get_base_stream` / `get_close_base_stream` / `set_close_base_stream` / `close_base_if_needed`.
  - 5 unit tests covering base stream access, flag toggle, read-through, and conditional close.

- **`gfilteroutputstream`** — GIO filter output stream. Mirrors `gio/gfilteroutputstream.h` / `gio/gfilteroutputstream.c`:
  - `FilterOutputStream` wrapping `OutputStream` with `Mutex`-protected `close_base_stream` flag (defaults `true`).
  - `new` / `get_base_stream` / `get_close_base_stream` / `set_close_base_stream` / `close_base_if_needed`.
  - 5 unit tests covering base stream access, flag toggle, write-through, and conditional close.

- **`ginitable`** — GIO initializable interface. Mirrors `gio/ginitable.h`:
  - `Initable` trait (`init(cancellable: Option<&GCancellable>) -> Result<(), Error>`).
  - 2 unit tests covering successful init and cancellation error propagation.

- **`glistmodel`** — GIO list model interface. Mirrors `gio/glistmodel.h`:
  - `ItemType` type alias (`String`) replacing `GType` for `no_std` compatibility.
  - `ListModel` trait: `get_item_type` / `get_n_items` / `get_item(position) -> Option<String>` / `items_changed` (provided no-op stub; signal wiring deferred to GObject signal system).
  - 5 unit tests covering n_items, get_item (hit/miss), empty model, and item_type.

- **`gliststore`** — GIO list store. Mirrors `gio/gliststore.h`:
  - `ListStore` plain struct with `Mutex`-protected `Vec<String>` and `item_type` field.
  - `new` / `append` / `insert` (no-op if OOB) / `remove` (no-op if OOB) / `remove_all` / `find` / `splice` (clamps position + removals) / `sort(F: Fn(&str, &str) -> Ordering)` / `n_items`.
  - Implements `ListModel`; all mutating methods take `&self` (interior mutability via `spin::Mutex`).
  - Items are `String` only (no generic type parameter; simplification over upstream `gpointer`).
  - 9 unit tests covering all operations including `splice`, `sort`, and `ListModel` delegation.

- **`gactiongroup`** — GIO action group interface. Mirrors `gio/gactiongroup.h`:
  - `ActionInfo` struct (`Clone` / `Debug`) — enabled / parameter_type / state_type / state_hint / state fields.
  - `ActionGroup` trait: required methods `has_action` / `list_actions` / `query_action` / `change_action_state` / `activate_action`; provided default impls for `get_action_enabled` / `get_action_parameter_type` / `get_action_state_type` / `get_action_state_hint` / `get_action_state` (all delegate to `query_action`).
  - 7 unit tests using a local `SimpleActionGroup` test harness.

- **`gactionmap`** — GIO action map interface. Mirrors `gio/gactionmap.h`:
  - `ActionEntry` struct (`Clone`) — name / parameter_type (`Option<String>` tag, e.g. `"s"`) / state (`Option<String>`); constructors `new` / `with_parameter` / `with_state`.
  - `ActionMap` trait: required `lookup_action(&str) -> Option<&dyn Action>` / `add_action(Box<dyn Action>)` / `remove_action(&str)`; provided `add_action_entries(&[ActionEntry])` (creates `SimpleAction` per entry, stateful entries seed state from `Variant::new_string`).
  - 7 unit tests covering lookup, add, remove, and `add_action_entries`.

- **`gasyncinitable`** — GIO async-initializable interface. Mirrors `gio/gasyncinitable.h`:
  - `AsyncInitable` trait: `init_async(io_priority: i32, cancellable: Option<&GCancellable>)` / `init_finish() -> Result<(), Error>`.
  - 2 unit tests covering success and failure paths via a `spin::Mutex`-backed test impl.

- **`gcharsetconverter`** — GIO charset converter. Mirrors `gio/gcharsetconverter.h`:
  - `CharsetConverter` struct: `to_charset` / `from_charset` (both `String`) + `Mutex`-protected `use_fallback: bool` and `num_fallbacks: u32`.
  - `new(to, from) -> Result<Self, Error>` (returns `NotSupported` for unsupported pairs) / `set_use_fallback` / `get_use_fallback` / `get_num_fallbacks` / `get_to_charset` / `get_from_charset`.
  - Implements `Converter` trait: identity passthrough (same charset) and UTF-8→ASCII with optional `?` fallback for non-ASCII bytes; all other charset pairs return `NotSupported`. `reset` zeroes `num_fallbacks`.
  - 7 unit tests covering construction, identity, UTF-8→ASCII with/without fallback, reset, and unsupported pair rejection.

- **`gicon`** — GIO icon discriminated union. Mirrors `gio/gicon.h`:
  - `Icon` enum: `Themed(ThemedIcon)` / `Bytes(BytesIcon)` / `Emblem(Emblem)` / `EmblemedIcon(EmblemedIcon)`.
  - `hash() -> u32` / `equal(&Self) -> bool` / `to_string() -> String` / `new_for_string(&str) -> Result<Self, IOErrorEnum>`.
  - Derives `PartialEq`, `Eq`, `Clone`, `Debug`.
  - 7 unit tests.

- **`gbytesicon`** — GIO bytes-backed icon. Mirrors `gio/gbytesicon.h`:
  - `BytesIcon` struct wrapping `Bytes`.
  - `new(Bytes) -> Self` / `bytes() -> &Bytes` / `hash() -> u32` / `equal(&Self) -> bool` / `to_string() -> String`.
  - 7 unit tests.

- **`gthemedicon`** — GIO themed icon by name. Mirrors `gio/gthemedicon.h`:
  - `ThemedIcon` struct: `init_names: Vec<String>` / `names: Vec<String>` / `use_default_fallbacks: bool`.
  - `new(&str) -> Self` / `new_with_default_fallbacks(&str) -> Self` / `new_from_names(&[&str]) -> Self`.
  - `names() -> &[String]` / `prepend_name(&str)` / `append_name(&str)` / `hash() -> u32` / `equal(&Self) -> bool` / `to_string() -> String`.
  - 14 unit tests.

- **`gemblem`** — GIO emblem (badge on an icon). Mirrors `gio/gemblem.h`:
  - `EmblemOrigin` enum: `Unknown=0` / `Device=1` / `LiveMetadata=2` / `Tag=3`; `from_i32(i32) -> Option<Self>` / `nick() -> &'static str`.
  - `Emblem` struct: `icon: Box<Icon>` / `origin: EmblemOrigin`; `new(Icon) -> Self` / `new_with_origin(Icon, EmblemOrigin) -> Self` / `icon() -> &Icon` / `origin() -> EmblemOrigin` / `hash()` / `equal()` / `to_string()`.
  - 11 unit tests.

- **`gemblemedicon`** — GIO icon with attached emblems. Mirrors `gio/gemblemedicon.h`:
  - `EmblemedIcon` struct: `icon: Box<Icon>` / `emblems: Vec<Emblem>` (sorted by hash).
  - `new(Icon, Option<Emblem>) -> Self` / `get_icon() -> &Icon` / `get_emblems() -> &[Emblem]` / `add_emblem(Emblem)` (insertion-sorted) / `clear_emblems()` / `hash()` / `equal()` / `to_string()`.
  - 12 unit tests.

- **`gsocketaddress`** — GIO socket address abstract type. Mirrors `gio/gsocketaddress.h`:
  - `SocketAddress` enum: `Inet(InetSocketAddress)` / `Unix(UnixSocketAddress)` / `Native(Vec<u8>)`.
  - `family() -> SocketFamily` / `native_size() -> usize` / `to_native(&mut [u8]) -> Result<(), IOErrorEnum>` / `new_from_native(&[u8]) -> Option<Self>` (classifies AF_INET/AF_INET6/AF_UNIX/unknown) / `to_string()`.
  - 18 unit tests.

- **`ginetsocketaddress`** — GIO IPv4/IPv6 socket address. Mirrors `gio/ginetsocketaddress.h`:
  - `InetSocketAddress` struct: address + port + flowinfo + scope_id.
  - `new(InetAddress, u16)` / `new_from_string(&str, u16)` / `new_with_ipv6_info(InetAddress, u16, u32, u32)`.
  - `address()` / `port()` / `flowinfo()` (0 on IPv4) / `scope_id()` (0 on IPv4) / `family()` / `native_size()` (16 IPv4 / 28 IPv6) / `to_native()` (serializes `sockaddr_in`/`sockaddr_in6` in network byte order) / `from_native()` (handles IPv4-mapped IPv6) / `to_string()` / `equal()`.
  - `SockaddrIn` / `SockaddrIn6` repr(C) structs.
  - 20 unit tests.

- **`gnetworkservice`** — GIO DNS SRV service resolver. Mirrors `gio/gnetworkservice.h`:
  - `NetworkService` struct: service + protocol + domain + scheme.
  - `new(&str, &str, &str) -> Self` / `service()` / `protocol()` / `domain()` / `scheme()` (defaults to service name when unset) / `set_scheme(&str)` / `to_string()` / `equal()`.
  - 11 unit tests.

- **`gproxyaddress`** — GIO proxy socket address. Mirrors `gio/gproxyaddress.h`:
  - `ProxyAddress` struct wrapping `InetSocketAddress` + protocol + dest_hostname + dest_port + username + password + uri + dest_protocol.
  - `new(InetAddress, u16, &str, &str, u16, Option<&str>, Option<&str>) -> Self` / `new_full(...)` (adds dest_protocol + uri).
  - `protocol()` / `destination_protocol()` / `destination_hostname()` / `destination_port()` / `username()` / `password()` / `uri()` + delegated `InetSocketAddress` accessors.
  - 13 unit tests.

- **`gunixsocketaddress`** — GIO Unix domain socket address. Mirrors `gio/gunixsocketaddress.h`:
  - `UnixSocketAddressType` enum: `Invalid` / `Anonymous` / `Path` / `Abstract` / `AbstractPadded`; `Default` = `Path`.
  - `UnixSocketAddress` struct: path bytes + address_type; `UNIX_PATH_MAX = 108`.
  - `new(&str)` / `new_with_type(&[u8], Option<usize>, UnixSocketAddressType)`.
  - `path()` / `path_len()` / `address_type()` / `is_abstract()` / `abstract_names_supported()` (true on Linux/no_std) / `family()` (always Unix) / `native_size()` / `to_native()` / `from_native()` / `to_string()` (non-printable bytes as `\xNN`) / `equal()`.
  - `SockaddrUn` repr(C) struct (110 bytes).
  - 24 unit tests.

- **`gbufferedinputstream`** — Buffered wrapper around `InputStream`. Mirrors `gio/gbufferedinputstream.h`:
  - `BufferedInputStream` struct: wraps `InputStream` base + `Mutex<Vec<u8>>` buffer + position + configurable buffer size (default 8192).
  - `new(base)` / `new_sized(base, size)`.
  - `get_buffer_size()` / `set_buffer_size()` / `get_available()` / `peek_buffer() -> Vec<u8>`.
  - `fill(count: i64, cancellable) -> Result<usize, Error>` (count -1 fills to capacity) / `read_byte()` / `read()` / `skip()` / `close()` / `is_closed()`.
  - 10 unit tests.

- **`gbufferedoutputstream`** — Buffered wrapper around `OutputStream`. Mirrors `gio/gbufferedoutputstream.h`:
  - `BufferedOutputStream` struct: wraps `OutputStream` base + `Mutex<Vec<u8>>` buffer + configurable size + auto-grow flag.
  - `new(base)` / `new_sized(base, size)`.
  - `get_buffer_size()` / `set_buffer_size()` / `get_auto_grow()` / `set_auto_grow()`.
  - `write()` (fills buffer; flushes to base when full; auto-grow expands buffer) / `flush()` / `close()` (flush then close base) / `get_base_stream()`.
  - 9 unit tests.

- **`gresolver`** — DNS/hostname resolution interface. Mirrors `gio/gresolver.h`:
  - `ResolverError` enum: `NotFound=0` / `Temporary=1` / `Internal=2`; `to_code() -> i32` / `resolver_error_quark() -> u32`.
  - `Resolver` trait: `lookup_by_name` / `lookup_by_address` / `lookup_service`.
  - `NoopResolver` struct: stub implementation always returning `Err(ResolverError::NotFound)` (no-std bare-metal default).
  - 6 unit tests.

- **`gsocket`** — Raw socket interface. Mirrors `gio/gsocket.h`:
  - `SocketType` enum: `Invalid=0` / `Stream=1` / `Datagram=2` / `Seqpacket=3`.
  - `SocketProtocol` enum (i32): `Unknown=-1` / `Default=0` / `Tcp=6` / `Udp=17` / `Sctp=132`.
  - `Socket` trait: `socket_type()` / `protocol()` / `is_connected()` / `is_closed()` / `close()` / `send()` / `receive()` / `get_timeout()` / `set_timeout()`.
  - `MockSocket` test impl: `VecDeque<u8>` behind `Mutex`; `inject(&self, data)` for test data; `new_stream()`.
  - Re-exports `SocketFamily` from `ginetaddress`.
  - 8 unit tests.

- **`gvolume`** — Removable volume interface. Mirrors `gio/gvolume.h`:
  - `MountUnmountFlags` enum: `None=0` / `Force=1`.
  - `Volume` trait: `get_name()` / `get_uuid()` / `can_mount()` / `can_eject()` / `should_automount()` / `get_sort_key()` (default impl).
  - `SimpleVolume` struct: concrete in-memory implementation.
  - 5 unit tests.

- **`gmount`** — Mounted volume interface. Mirrors `gio/gmount.h`:
  - `Mount` trait: `get_name()` / `get_uuid()` / `can_unmount()` / `can_eject()` / `get_default_location()`.
  - `SimpleMount` struct: concrete in-memory implementation.
  - 5 unit tests.

- **`gdrive`** — Storage drive interface. Mirrors `gio/gdrive.h`:
  - `DriveStartFlags` enum: `None=0`.
  - `Drive` trait: `get_name()` / `get_identifier()` / `enumerate_identifiers()` / `has_volumes()` / `can_eject()` / `can_poll_for_media()` / `is_media_removable()` / `is_media_check_automatic()`.
  - `SimpleDrive` struct: concrete in-memory implementation.
  - 5 unit tests.

- **`gsettings`** — Application settings store. Mirrors `gio/gsettings.h`:
  - `SettingsValue` enum: `Bool(bool)` / `Int(i32)` / `Int64(i64)` / `Uint(u32)` / `Uint64(u64)` / `Double(f64)` / `Str(String)` / `Strv(Vec<String>)`.
  - `Settings` struct: `Mutex<BTreeMap<String, SettingsValue>>` + `schema_id: String`.
  - `new(schema_id)` / `get_schema_id()` / typed `get_*/set_*` for all 8 variant types / `reset(key)` / `list_keys() -> Vec<String>`.
  - 10 unit tests.

- **`gvfs`** — Virtual filesystem interface. Mirrors `gio/gvfs.h`:
  - `Vfs` trait: `is_active()` / `get_file_for_path()` / `get_file_for_uri()` / `get_supported_uri_schemes()` / `parse_name()`.
  - `LocalVfs` struct: default implementation; `new()` / `Default` impl.
  - 7 unit tests.

- **`gfileenumerator`** — Directory listing enumerator. Mirrors `gio/gfileenumerator.h`:
  - `FileEnumerator` struct: `Mutex`-protected list of `FileInfo` entries + closed/pending flags + container path.
  - `new(container, entries)` / `next_file()` / `close()` / `is_closed()` / `has_pending()` / `set_pending()` / `get_container()` / `get_child(info)` / `iterate()`.
  - 8 unit tests.

- **`gfilemonitor`** — File/directory change monitor. Mirrors `gio/gfilemonitor.h`:
  - `FileMonitorEvent` enum: `Changed` / `ChangesDoneHint` / `Deleted` / `Created` / `AttributeChanged` / `PreUnmount` / `Unmounted` / `Moved` / `MovedIn` / `MovedOut` / `Renamed`.
  - `FileMonitor` struct: cancellation flag + rate-limit + `Mutex<Vec<FileMonitorEvent>>` event log.
  - `new()` / `cancel()` / `is_cancelled()` / `set_rate_limit()` / `get_rate_limit()` / `emit_event()` / `get_events()`.
  - 5 unit tests.

- **`gmountoperation`** — Interactive mount credential dialog state. Mirrors `gio/gmountoperation.h`:
  - `PasswordSave` enum: `Never` / `ForSession` / `Permanently`.
  - `MountOperationResult` enum: `Handled` / `Aborted` / `Unhandled`.
  - `AskPasswordFlags` bitflags for interactive prompt controls.
  - `MountOperation` struct: `Mutex`-protected credentials (username/password/domain/anonymous/choice/password_save) + TCRYPT params (hidden_volume/system_volume/pim).
  - `new()` / `get_*/set_*` for all credential fields / `reply(result)`.
  - 10 unit tests.

- **`gsimpleactiongroup`** — Concrete `ActionGroup` + `ActionMap`. Mirrors `gio/gsimpleactiongroup.h`:
  - `SimpleActionGroup` struct: `Mutex<Vec<Box<dyn Action>>>` named actions.
  - Implements `ActionGroup` (`has_action` / `list_actions` / `query_action` / `change_action_state` / `activate_action`) and `ActionMap` (`lookup_action` / `add_action` / `remove_action`).
  - `new()` / `add_action_entries()`.
  - 9 unit tests.

- **`gpropertyaction`** — Property-bound `Action`. Mirrors `gio/gpropertyaction.h`:
  - `PropertyAction` struct: `Mutex<Variant>` state bound to a named property.
  - Implements `Action` trait (delegating `get_name`/`get_enabled`/`get_parameter_type`/`get_state_type`/`get_state_hint`/`get_state`/`change_state`/`activate`).
  - `new(name, initial_state)` / `get_property_name()`.
  - 7 unit tests.

- **`gzlibcompressor`** — zlib/gzip compression `Converter`. Mirrors `gio/gzlibcompressor.h`:
  - `ZlibCompressorFormat` enum: `Zlib` / `Gzip` / `Raw`.
  - `ZlibCompressor` struct: passthrough stub (actual zlib not available in no-std; interface correct).
  - `new(format, level)` / `get_format()` / `get_level()` / `get_file_info()` / `set_file_info()` / `get_os()` / `set_os()`.
  - Implements `Converter` (`convert` / `reset`).
  - 7 unit tests.

- **`gzlibdecompressor`** — zlib/gzip decompression `Converter`. Mirrors `gio/gzlibdecompressor.h`:
  - `ZlibDecompressor` struct: passthrough stub implementing `Converter`.
  - `new(format)` / `get_format()` / `get_file_info()`.
  - Implements `Converter` (`convert` / `reset`).
  - 5 unit tests.

- **`gfileinputstream`** — File-backed `InputStream` with seek. Mirrors `gio/gfileinputstream.h`:
  - `FileInputStream` struct: in-memory byte buffer + position cursor + closed flag.
  - `from_data(bytes)` / `tell()` / `can_seek()` / `seek(offset, seek_type)` / `query_info(attributes)` / `read(buf, cancellable)` / `close()` / `is_closed()`.
  - 9 unit tests.

- **`gfileoutputstream`** — File-backed `OutputStream` with seek/truncate/etag. Mirrors `gio/gfileoutputstream.h`:
  - `FileOutputStream` struct: in-memory buffer + seek + etag support.
  - `new()` / `from_data(bytes)` / `tell()` / `can_seek()` / `can_truncate()` / `seek()` / `truncate()` / `query_info()` / `get_etag()` / `write()` / `close()` / `is_closed()` / `get_data()`.
  - 9 unit tests.

- **`gfileiostream`** — Bidirectional file stream with seek/truncate. Mirrors `gio/gfileiostream.h`:
  - `FileIOStream` struct: single in-memory buffer supporting both read and write + full seek/truncate.
  - `new()` / `from_data(bytes)` / `tell()` / `can_seek()` / `can_truncate()` / `seek()` / `truncate()` / `query_info()` / `get_etag()` / `read()` / `write()` / `close()` / `is_closed()` / `get_data()`.
  - 8 unit tests.

- **`gfiledescriptorbased`** — File-descriptor interface. Mirrors `gio/gfiledescriptorbased.h`:
  - `FileDescriptorBased` trait: single method `get_fd(&self) -> i32`.
  - Implemented by types that wrap OS file descriptors (stubbed for no-std).
  - 3 unit tests.

- **`gloadableicon`** — Loadable icon interface. Mirrors `gio/gloadableicon.h`:
  - `LoadableIcon` trait: `load(size: i32, cancellable: Option<&GCancellable>) -> Result<(InputStream, Option<String>), Error>`.
  - Returns an `InputStream` plus optional MIME-type string.
  - 2 unit tests.

- **`gfileicon`** — File-path icon implementing `LoadableIcon`. Mirrors `gio/gfileicon.h`:
  - `FileIcon` struct: wraps a `File` path + optional cached byte data.
  - `new(file)` / `get_file()` / `set_data(bytes)`.
  - Implements `LoadableIcon::load` (returns `MemoryInputStream` of cached bytes).
  - 4 unit tests.

- **`gfilenamecompleter`** — Tab-completion for file paths. Mirrors `gio/gfilenamecompleter.h`:
  - `FilenameCompleter` struct: `Mutex<Vec<String>>` entry list + `dirs_only` flag.
  - `new()` / `add_entry(path)` / `get_completion_suffix(prefix) -> Option<String>` (longest common suffix) / `get_completions(prefix) -> Vec<String>` / `set_dirs_only(bool)`.
  - 8 unit tests.

- **`gobject`** — GObject base class with properties, signals, and weak refs. Mirrors `gobject/gobject.h`:
  - `ObjectFlags` struct / `GObject` struct: ref-counted (Arc-based), property map, signal connections, weak-ref list, user data map.
  - `new()` / `new_with_params()` / `type_id()` / `type_name()` / `ref_` / `unref` / `ref_count()` / `is_floating()` / `force_floating()` / `ref_sink()`.
  - `install_properties()` / `get_property()` / `set_property()` / `list_properties()`.
  - `connect_signal()` / `emit_signal()` / `freeze_notify()` / `thaw_notify()`.
  - `add_weak_ref()` / `clear_weak_refs()` / `set_data()` / `get_data()` / `remove_data()`.
  - `PropertyBinding` struct: `new(src, src_prop, dst, dst_prop)` / `sync()`.
  - 9 unit tests.

- **`gparamspec`** — Property parameter specification. Mirrors `gobject/gparamspec.h`:
  - `ParamSpec` struct: typed property descriptor (name, flags, default value).
  - Typed constructors: `boolean` / `int` / `uint` / `string` / `double` / `float` / `enum_` / `flags` / `int64` / `uint64` / `char` / `uchar` / `long` / `ulong` / `object` / `pointer`.
  - `is_readable()` / `is_writable()` / `is_construct_only()` / `get_default_value()` / `value_validate()`.
  - Free fns: `install_properties()` / `find_property()` / `find_property_by_id()` / `property_names()`.
  - Re-exports `ParamFlags` from `gtype`. `ParamID` type alias.
  - 6 unit tests.

- **`gsignal`** — Signal registration, connection, and emission. Mirrors `gobject/gsignal.h`:
  - `SignalFlags` / `ConnectFlags` / `SignalQuery` structs; `SignalID` / `HandlerID` / `SignalCallback` type aliases.
  - `signal_new()` / `signal_lookup()` / `signal_query()` / `signal_name()`.
  - `signal_connect()` / `signal_connect_by_name()` / `signal_handler_disconnect()` / `signal_handler_is_connected()` / `signal_handler_block()` / `signal_handler_unblock()`.
  - `signal_emit()` / `signal_emit_by_name()` / `signal_list_ids()` / `signal_n_handlers()` / `signal_handlers_disconnect_all()`.
  - 8 unit tests.

- **`gtype`** — GObject type system and type registry. Mirrors `gobject/gtype.h`:
  - `GType` type alias; `GTypeFundamentalFlags` / `GTypeFlags` / `ParamFlags` structs; `GTypeValueTable` / `GTypeInfo` / `TypeQuery` structs.
  - 21 pre-registered fundamental types via `type_init()`.
  - `type_from_name()` / `type_name()` / `type_parent()` / `type_fundamental()` / `type_is_a()` / `type_depth()` / `type_children()` / `type_interfaces()`.
  - `type_register_fundamental()` / `type_register_static()` / `type_register_static_simple()`.
  - `type_is_classed()` / `type_is_instantiatable()` / `type_is_abstract()` / `type_is_final()` / `type_add_interface()` / `type_query()` / `type_get_all()`.
  - 10 unit tests.

- **`gvalue`** — Polymorphic value container. Mirrors `gobject/gvalue.h`:
  - `GValue` struct: holds any registered `GType` value as a tagged union.
  - `new()` / `init(type_)` / `for_type(type_)` / `reset()` / `clear()` / `value_type()` / `holds(type_)` / `copy_from()`.
  - Typed `set_*/get_*` pairs for: `boolean` / `int` / `uint` / `int64` / `uint64` / `float` / `double` / `char` / `uchar` / `long` / `ulong` / `string` / `pointer` / `enum_` / `flags` / `object` / `boxed`.
  - Free fns: `value_new_boolean` / `value_new_int` / `value_new_string` / `value_new_double` / `value_new_uint` / `value_new_int64` / `value_new_uint64` / `value_new_float` / `value_new_char` / `value_new_enum` / `value_new_flags` / `value_new_pointer` / `value_new_object` / `value_new_boxed`.
  - `default_value_table_for(type_)` helper.
  - 8 unit tests.

- **`gstring`** — Growable string buffer. Mirrors `glib/gstring.h`:
  - `GString` struct: `alloc::string::String` wrapper with GLib-compatible mutating API.
  - `new(init)` / `new_take(s)` / `new_len(init, len)` / `sized_new(dfl_size)`.
  - `as_ptr()` / `as_str()` / `as_bytes()` / `len()` / `is_empty()` / `allocated_len()`.
  - `append(s)` / `append_len(s, len)` / `append_c(c)` / `truncate(len)` / `set_size(len)` / `into_inner()` / `free(free_segment)`.
  - `equal()` / `hash()`.
  - 9 unit tests.

### Deferred

- Remaining GIO submodules: GDBus server, GDBus object manager, GDBus auth, etc.
- `g_io_error_from_win32_error` (Windows error → IOErrorEnum) —
  deferred; not applicable to bare-metal RustOS.

### Completed (previously deferred — GApplication / GDBus connection / proxy)

- **`gapplication`** — `Application` with `ApplicationFlags` (all upstream
  `G_APPLICATION_*` constants), embedded `SimpleActionGroup` (full
  `ActionGroup`/`ActionMap` delegation), per-instance signal registry
  (`startup`/`activate`/`open`/`command-line`/`shutdown`), real hold/release
  counting (`release`→0 calls `quit`), `run`/`run_default` lifecycle state
  machine (register → startup → activate/open/command-line → `MainLoop::run` →
  shutdown), local-only `register`, in-app `Notification` registry
  (`send_notification`/`withdraw_notification`), `application_id_is_valid`.
- **`gdbusconnection`** — `DBusConnection` with `DBusTransport` trait,
  `LoopbackTransport` (fully functional in-process bus: method-call dispatch,
  signal fan-out, monotonic serials), `NoDbusTransport` (bare-metal default
  returning `NotSupported`), `signal_subscribe`/`signal_unsubscribe`/
  `signal_emit`, `register_object`/`unregister_object` with path validation,
  `call`/`call_sync`, `send_message`/`send_message_with_reply_sync`,
  `new_for_address_sync("loopback:")`.
- **`gdbusproxy`** — `DBusProxy` with `DBusProxyFlags`, property cache
  populated via real `org.freedesktop.DBus.Properties.GetAll` round-trip,
  `PropertiesChanged` subscription keeping cache fresh, `call`/`call_sync`,
  `connect_signal`/`connect_properties_changed`, `close` unsubscribes
  connection subscriptions; `new_for_bus` supports loopback peer only on bare
  metal.

### Completed (previously deferred)

- **`g_dbus_node_info_new_for_xml` + XML generation** —
  `dbus_node_info_new_for_xml` parses D-Bus introspection XML by
  recursively walking the `crate::markup` parse tree into the
  `DBus*Info` structs (requires a single `<node>` root, the standard
  D-Bus introspection shape). Generators
  `dbus_node_info_generate_xml` / `dbus_interface_info_generate_xml` /
  `dbus_annotation_info_generate_xml` emit indented XML. (The
  `crate::markup` API is a tree builder rather than a SAX
  `GMarkupParseContext`, so the walker is self-contained in
  `gdbusintrospection.rs`.)
- **`g_dbus_interface_info_cache_build` / `_release`** — a global
  `spin::Mutex<BTreeMap<String, Arc<DBusInterfaceInfo>>>` keyed by
  interface name, with `dbus_interface_info_cache_build` (insert,
  no-op if already present) / `_release` / `_lookup`.
- **`g_dbus_error_set_dbus_error` / `_valist`** — build a
  `glib_native::Error` from a D-Bus error name + message with a
  printf-formatted detail piece, reusing
  `dbus_error_new_for_dbus_error`. The `_valist` form models the C
  `va_list` as `core::fmt::Arguments`; the printf helper supports
  `%s` / `%%` (sufficient for D-Bus error messages) since
  `printf.rs`'s public API only accepts `&'static str`. Both take
  `&mut Error` and overwrite unconditionally (the natural Rust mapping
  of upstream's `GError **` out-parameter).

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
- ~~GValue transform functions between types.~~ **Done** — see
  `gvaluetransform` module: `value_register_transform_func` /
  `value_type_transformable` / `value_type_compatible` /
  `value_transform` + built-in numeric/bool/string transforms.
- GType plugin system (dynamic type registration).
- C ABI compatibility (`extern "C"` wrappers, `GTypeInstance` layout).

### Completed (previously deferred)

- **GValue transform functions** — `value_register_transform_func` /
  `value_can_transform` / `value_transform` backed by a
  `spin::Once<Mutex<BTreeMap<(GType, GType), TransformFunc>>>` registry.
  `value_transform` does an identity copy when `src_type == dest_type`, consults
  the registry otherwise, and returns `false` (never panics) on a miss —
  matching upstream's `gboolean`. The `GTypeValueTable` transform-hook fallback
  is not modelled (no native hook yet).
- **GParamSpec pool and override** — `ParamSpecPool`
  (`RwLock<BTreeMap<GType, Vec<Arc<ParamSpec>>>>`) with `new`/`insert`/`lookup`/
  `remove`/`list`/`list_owned` + free-function wrappers, and
  `param_spec_override` / `param_spec_override_resolve`. `ParamSpec` gained an
  `override_origin: Option<(GType, String)>` field (wired into all 16
  constructors); overrides carry placeholder `value_type`/`flags` and resolve the
  parent's metadata lazily via the pool.
- **GObject closure system (`GClosure`)** — `Closure` (ref-counted via the
  wrapping `Arc<Closure>`, `Weak` self-ref to avoid a leak cycle) with
  `invoke`/`ref_`/`sink`/`is_floating`/`invalidate`/`is_invalidated`/
  `set_marshal`/`add_notify`, plus `closure_new`/`cclosure_new`/`closure_invoke`
  and `GObject::connect_closure` (a thin adapter delegating to the existing
  `signal_connect_by_name`, no `gsignal.rs` edits).
- **GInterface vtable initialization and dispatch** — `InterfaceVTable`
  (type-erased `Box<dyn Any + Send + Sync>` + instance/interface `GType`, with
  `downcast_ref`), `InterfaceInfo`, a `Once<RwLock<BTreeMap<(GType, GType),
  Arc<InterfaceVTable>>>>` registry, `type_add_interface_static` (runs
  `interface_init` then `class_init`), `type_interface_peek` /
  `type_interface_is_a` / `type_peek_vtable`. `interface_finalize` is kept for
  API parity but not dispatched (no type-finalization model in `no_std`).
- **GType plugin system (dynamic type registration)** — `TypePlugin` trait
  (`complete_type_info` / `complete_interface_info` + default no-op
  `use_`/`unuse`), `type_register_dynamic` (placeholder type + plugin stored in
  a `Once<Mutex<…>>` registry), `type_ensure` (idempotent lazy completion),
  `type_plugin_use` / `type_plugin_unuse` (use-count registry, floored at zero),
  `type_plugin_complete_interface_info`. `TypeNode` gained
  `is_dynamic`/`dynamic_info_completed` fields.

## Phase 7 detail (partial)

### Modules

- **`date`** — `Date` with pure math (day/month/year arithmetic, weekday calculation).
- **`timezone`** — `TimeZone` with fixed offsets and embedded IANA names (`new_iana`),
  simplified DST rules for common US/EU/AU zones.
- **`datetime`** — `DateTime` with UTC arithmetic, formatting, parsing, `to_timezone`.
- **`varianttype`** — `VariantType` parser and validator.
- **`variant`** — `Variant` value container, `VariantBuilder`, parser.
- **`unicode`** — Types, enums, combining class, basic `unichar` functions.
- **`utf8`** — UTF-8 encoding/decoding, `utf8_get_char`, `utf8_next_char`.

### Deferred

- Full IANA zoneinfo/TZif database (filesystem-backed).
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
  Real process spawning is implemented on RustOS via `glib_spawn` (VFS read → `process_manager::fork` → ELF `exec`, child `cwd`, scheduler registration).
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

**254 modules** ported across Phases 1–13, all wired into RustOS:

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
| 9 | gtype, gvalue, gparamspec, gsignal, gobject, gvaluetransform | Partial (6) |
| 10 | gmodule | Partial (1) |
| 11 | gfileattribute, gdbusintrospection, gdbuserror, gioerror, gnotification, gsrvtarget, ginetaddress, ginetaddressmask, ginetsocketaddress, gnetworkaddress, gnetworkservice, gproxyaddress, gsocketaddress, gunixsocketaddress, gbytesicon, gthemedicon, gicon, gemblem, gemblemedicon, ginputstream, goutputstream, giostream, gfile, gseekable, gdatainputstream, gdataoutputstream, gasyncresult, gpermission, gsimplepermission, gconverter, gaction, gsimpleaction, gfilterinputstream, gfilteroutputstream, ginitable, glistmodel, gliststore, gactiongroup, gactionmap, gasyncinitable, gcharsetconverter, gzlibcompressor, gzlibdecompressor, gsimpleactiongroup, gpropertyaction, gmountoperation, gfileinputstream, gfileoutputstream, gfileiostream, gfiledescriptorbased, gloadableicon, gfileicon, gfilenamecompleter, gfileenumerator, gfilemonitor, gvfs, gconverterinputstream, gconverteroutputstream, gsocketconnectable, gpollableinputstream, gpollableoutputstream, gresource, gcontenttype, gmenu, gappinfo, gapplication, gmenumodel, gsettingsschema, gdbusmessage, gcredentialsmessage, gnetworkmonitor, gtcpwrapperconnection, gpowerprofilemonitor, gtlscertificate, gsocketconnection, gdbusmethodinvocation, gsettingsbackend, gproxy, gsocketaddressenumerator, gtlsbackend, gtlsclientconnection, gtlsinteraction, gdtlsconnection, gtlsdatabase, gtlsfiledatabase, gtlsserverconnection, gdatagrambased, gvolumemonitor, gtask, gsocketclient, gsocketlistener, gsimpleiostream, gsubprocess, gsubprocesslauncher, gtcpconnection, gunixconnection, gsocketcontrolmessage, gtlsconnection, gsocketservice, gthreadedsocketservice, gproxyresolver, gdtlsclientconnection, gdtlsserverconnection, gdbusserver, gappinfomonitor, gdbusconnection, gdbusproxy, gdbusinterface, gdbusobject, gdbusobjectskeleton, gdbusobjectmanager, gdbusmenumodel, gdbusnameowning, gdbusnamewatching, gdbusutils, gdbusauthobserver, gcancellable, gsimpleasyncresult, gtlspassword, gremoteactiongroup, gdbusactiongroup, gactiongroupexporter, gapplicationcommandline, gdesktopappinfo, gsimpleproxyresolver, gdummyproxyresolver, gdummytlsbackend, gsocketinputstream, gsocketoutputstream, gnativesocketaddress, gthreadedresolver, giomodule, gioscheduler, gunixinputstream, gunixoutputstream, gunixfdlist, gunixfdmessage, gunixcredentialsmessage, gdelayedsettingsbackend, gmemorymonitor, gdbusinterfaceskeleton, gdbusobjectproxy, gdbusobjectmanagerclient, gdbusobjectmanagerserver, gdbusaddress, gmenuexporter, gdebugcontroller, gnotificationbackend, gproxyaddressenumerator, gresourcefile, ghttpproxy, gsocks4aproxy, gsocks4proxy, gsocks5proxy, gnetworkmonitorbase, gpollfilemonitor, gportalsupport, gopenuriportal, gasynchelper, gcontextspecificgroup, gpollableutils, gdbusauth, gdbusauthmechanism, gdbusauthmechanismanon, gdbusauthmechanismexternal, gdbusauthmechanismsha1, gdebugcontrollerdbus, gtestdbus, gsandbox, gdummyfile, giptosmessage, gipv6tclassmessage, gnativevolumemonitor, gunionvolumemonitor, gunixmount, gunixmounts, gunixvolume, gunixvolumemonitor, gregistrysettingsbackend, glocalfile, glocalfileenumerator, glocalfileinfo, glocalfileinputstream, glocalfileoutputstream, glocalfileiostream, glocalfilemonitor, glocalvfs, gmemorymonitordbus, gmemorymonitorpoll, gmemorymonitorportal, gmemorymonitorpsi, gpowerprofilemonitordbus, gpowerprofilemonitorportal, gproxyresolverportal, gtrashportal, gdocumentportal, gnetworkmonitornetlink, gnetworkmonitornm, gnetworkmonitorportal, gapplicationimpl, gdbusdaemon, gioenums, giotypes, thumbnail-verify | Partial (204) |

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
- **GInetAddressMask** — IPv4 `/24` mask parse + fields + matches
  (within/outside), different family → no match, full-length mask
  (no `/prefix`) → 32 for IPv4 + `to_string` omits `/length`, IPv6
  `/32` mask + matches, error cases (non-ip parse, length too long,
  bits beyond prefix), constructor bits-beyond-prefix check, `equal`
  (same/different-length)
- **GNetworkAddress** — `new` + accessors, `new_loopback` (localhost),
  `parse` (plain hostname → default port, host:port, bracketed IPv6
  with port, unescaped IPv6, error cases: empty port, invalid port,
  unclosed bracket), `parse_uri` (http://example.com:8080/path →
  scheme/host/port, no-port → default, invalid URI rejected), `equal`
  (same/different-port)
- **GValueTransform** — `init_builtin_transforms`, int→uint, int→double,
  int→string, int→bool (non-zero→true, zero→false), bool→string
  (TRUE/FALSE), same-type copy via `value_type_compatible`,
  `value_type_transformable` checks (true for int→uint/int→double/
  int→int, false for string→float)
- **GInetSocketAddress** — `new` + accessors (address/port/family),
  `new_from_string` (valid + invalid rejection), IPv6 with flowinfo/
  scope_id, flowinfo/scope_id return 0 for IPv4, `native_size` (16 for
  IPv4, 28 for IPv6), `to_native`+`from_native` roundtrip (IPv4 + IPv6
  with flowinfo/scope_id preservation), `to_native` no-space error,
  `to_string` (IPv4 `addr:port`, IPv6 `[addr]:port`, scope_id
  `[addr%scope]:port`), `equal`
- **GSocketAddress** — Inet variant family/native_size/to_string,
  `new_from_native` (IPv4 → Inet with field validation, IPv6 → Inet
  with family check, unknown family → Native variant, AF_UNSPEC →
  None)
- **GUnixSocketAddress** — `new` + path/accessors/family, `new_with_type`
  (anonymous/abstract/path_len), `native_size` (Path=110, Anonymous=2),
  `to_native`+`from_native` roundtrip (path + abstract with type
  preservation), `to_native` no-space error, `to_string` (path +
  anonymous), `equal`, SocketAddress::Unix variant family/to_string,
  SocketAddress::new_from_native (Unix → path validation)
- **GNetworkService** — `new` + accessors (service/protocol/domain),
  default scheme (= service name), `set_scheme`, `to_string`
  (`(service, protocol, domain, scheme)`), `equal` (same/different
  scheme/protocol/domain)
- **GProxyAddress** — `new` + accessors (protocol/dest_hostname/
  dest_port/username/password), optional fields (uri/dest_protocol
  default None), delegated accessors (address/port/family/native_size/
  to_string/to_native), `new_full` (with dest_protocol + uri), `equal`
  (same/different protocol/dest/port/username)
- **GThemedIcon** — `new` + names (with symbolic variant), `new_with_default_fallbacks`
  (shortening at `-`: gnome-dev-cdrom-audio → gnome-dev-cdrom → gnome-dev → gnome),
  no-fallbacks check, `new_from_names` (multi), `prepend_name`/`append_name`,
  `equal` (same/different), `to_string`
- **GBytesIcon** — `new` + `bytes` accessor, `to_string` (`"bytes"`),
  `equal` (same/different), `hash` consistency
- **GIcon** — Themed/Bytes/Emblem/EmblemedIcon enum variants, `equal` (same type/different type),
  `new_for_string` (single name → Themed, multi `. name1 name2` → Themed,
  empty → error), `hash` consistency, Emblem/EmblemedIcon variant equal + hash
- **GEmblem** — `new_with_origin` + `origin`/`icon` accessors, `equal` (same/different origin),
  `hash` consistency, `EmblemOrigin::from_i32` (valid/invalid)
- **GEmblemedIcon** — `new` with/without emblem, `get_icon`/`get_emblems`, `add_emblem`/`clear_emblems`,
  `equal` (same/different emblem), `hash` consistency, `to_string` (icon + emblem names)
- **GInputStream** — `MemoryInputStream::new_from_bytes` + `read_all` (data match),
  `MemoryInputStream` close/pending/cancellation/skip/add_bytes
- **GOutputStream** — `MemoryOutputStream::new_resizable` + `write_all` + `get_data` (via downcast),
  `splice` (input→output, data match), close/pending/cancellation/steal_as_bytes
- **GIOStream** — `new` (input+output), `get_input_stream`/`get_output_stream`, `close` (closes both),
  `is_closed`/`set_pending`/`clear_pending`, close with cancellation
- **GFile** — `new_for_path` + `get_path`/`get_basename`/`get_parent`, `new_for_uri`,
  `FileInfo` setters/getters (`set_size`/`set_file_type`/`set_name`/`set_attribute_string`),
  `FilePlatform` mock (read/create/query_info via registered platform)
- **GSeekable** — `SeekType` enum values (Cur=0/Set=1/End=2),
  seek on `MemoryInputStream` (Set/Cur/End, data match after seek),
  seek+truncate on `MemoryOutputStream` (write→seek→overwrite→truncate)
- **GDataInputStream** — `read_uint16` (BE), `read_line` (LF, returns `Option<String>`),
  byte order switch (LE), `set_byte_order`/`get_byte_order`,
  `set_newline_type`/`get_newline_type` (CrLf)
- **GDataOutputStream** — `put_uint16`/`put_uint32`/`put_string` (BE, data match via downcast),
  byte order switch (LE)
- **GPermission** — default state (all false), `impl_update` (allowed/can_acquire/!can_release),
  `acquire` succeeds when can_acquire, `release` fails when !can_release
- **GSimplePermission** — `new(true)`/`new(false)`, `get_allowed` matches
- **GConverter** — `ConverterFlags` enum values (NoFlags=0/InputAtEnd=1/Flush=2),
  `ConverterResult` enum values (Error=0/Converted=1/Finished=2/Flushed=3)
- **GAction** — `action_name_is_valid` (valid/invalid), `action_parse_detailed_name`
  (string target), `action_print_detailed_name` round-trip
- **GSimpleAction** — `new` (name+enabled defaults), `set_enabled`,
  `new_stateful` (boolean state), `change_state`
- **GFilterInputStream** — default `close_base_stream` (true),
  `set_close_base_stream` (false), `get_base_stream` read (data match)
- **GFilterOutputStream** — default `close_base_stream` (true),
  `set_close_base_stream` (false), `get_base_stream` write (data match via downcast)
- **GListStore / GListModel** — `append` (3 items), `get_n_items`, `get_item`,
  `remove`, `find`, `ListModel` trait dispatch (`get_n_items` via `&dyn ListModel`)
- **GCharsetConverter** — identity convert (UTF-8→UTF-8, Finished result),
  UTF-8→ASCII with fallback (non-ASCII bytes replaced with `?`, `num_fallbacks` count)
- **GZlibCompressor** — `new` (Gzip format, level 6), passthrough convert
  (Finished result, data match)
- **GSimpleActionGroup** — `add_action` + `has_action`, `list_actions` (count),
  `remove_action` (action gone)
- **GMountOperation** — `set_username`/`get_username`, `set_password_save`/`get_password_save`
- **GFileInputStream** — `read` (data match), `seek` (Set) + `read` (data match after seek)
- **GFileOutputStream** — `write` (data match), `seek` + `write` (overwrite at position),
  `truncate` (data match)
- **GFileIOStream** — `read` (data match), `seek` + `write` (overwrite at position, data match)
- **GFileIcon** — `get_file` (path match), `set_data` + `load` (type + data match via `read_all`)
- **GFilenameCompleter** — `get_completions` (count), `get_completion_suffix` (no match),
  `set_dirs_only` + `get_completions` (filtered to dirs only)
- **GFileEnumerator** — `next_file` (2 entries in order), exhausted (None)
- **GFileMonitor** — `emit_event` (Changed event recorded), `cancel` (is_cancelled)
- **GVfs** — `is_active`, `get_file_for_path` (path match), `get_supported_uri_schemes` (contains "file")
- **GMemoryOutputStream** — `write` (data match), `get_data_size` (count), `steal_as_bytes` (data + reset)
- **GConverterInputStream** — `read` (data match), `get_converter_name` (name match)
- **GConverterOutputStream** — `write` (data match via `get_data`)
- **GSocketConnectable** — `to_string` (host:port), `enumerate` (empty → None)
- **GResource** — `lookup_data` (data match), `get_info` (size match), `enumerate_children` (count)
- **GContentType** — `equals` (match), `is_unknown` (octet-stream), `guess` (filename→text/plain), `can_be_executable`
- **GMenu** — `append` (2 items), item `get_label`/`get_action` (content match)
- **GAppInfo** — `get_id`/`get_name` (match), `get_executable` (match)
- **GMenuModel** — `append` (1 item), `get_item_attribute_value` (label match)
- **GSettingsSchema** — `add_schema` + `lookup` (found), `has_key` (true)
- **GDBusMessage** — `new_signal` (type=Signal), `get_header` (path match)
- **GNetworkMonitor** — `get_network_available` (true), `get_connectivity` (Full)
- **GPowerProfileMonitor** — `get_power_saver_enabled` (false), `get_profile` (Performance)
- **GTlsCertificate** — `new_from_pem` + `is_valid` (true), `get_pem` (non-empty)
- **GFileInfo** — `set_name`/`get_name` (match), `set_file_type`/`get_file_type` (Regular), `set_size`/`get_size` (1024)
- **GMemoryInputStream** — `new_from_bytes` + `read` (5 bytes, "hello" match)
- **GDBusMethodInvocation** — `get_method_name` (match), `return_value` + `has_reply` (true)
- **GSettingsBackend** — `write` + `read` (match), `get_writable` (true)
- **GProxy** — `get_protocol` ("http"), `supports_hostname` (true)
- **GTlsBackend** — `supports_tls` (true)
- **GTlsInteraction** — `set_password` + `ask_password` (Handled), `get_value` (match)
- **GTlsDatabase** — `add_anchor` + `n_anchors` (1)
- **GTlsServerConnection** — `set_authentication_mode`/`get_authentication_mode` (Required)
- **GDatagramBased** — `inject` + `receive` (data match)
- **GVolumeMonitor** — `add_volume` + `volume_count` (1)
- **GTask** — `set_name`/`get_name` (match)
- **GSocketClient** — `get_timeout` (0 default)
- **GProxyResolver** — `is_supported` (true)
- **GTlsConnection** — `get_require_close_notify` (false default), `set_require_close_notify` (true)
- **GDBusServer** — `new` + `get_client_address` (non-empty)
- **GAppInfoMonitor** — `register_app` + `app_count` (1)
- **GDtlsClientConnection** — `set_server_identity`/`get_server_identity` (match)
- **GDtlsConnection** — `is_handshake_done` (false initial)
- **GDtlsServerConnection** — `set_authentication_mode`/`get_authentication_mode` (Requested)
- **GSocketService** — `start` + `is_active` (true)
- **GThreadedSocketService** — `new(4)` + `start` + `is_active` (true)
- **GTlsClientConnection** — `new` + `get_server_identity` (match)
- **GTlsFileDatabase** — `new` + `get_anchors` (non-empty)
- **GCredentials** — `new` + `get_unix_pid` (exercised)
- **GCredentialsMessage** — `new_with` + `get_credentials` (exercised)
- **GPropertyAction** — `new` + `get_property_name` (match)
- **GSimpleIOStream** — `new` + `is_closed` (false)
- **GSocketAddressEnumerator** — `new` + `next(None)` (Some)
- **GSocketConnection** — `new` + `is_closed` (false)
- **GSocketControlMessage** — `new` + `get_level` (1), `get_data` (len 4)
- **GSocketListener** — `set_backlog`/`get_backlog` (5)
- **GSubprocess** — `new` + `is_running` (true)
- **GSubprocessLauncher** — `set_cwd`/`get_cwd` (match)
- **GTcpConnection** — `new` + `is_closed` (false)
- **GTcpWrapperConnection** — `new` + `is_closed` (false)
- **GUnixConnection** — `new` + `is_closed` (false), `get_peer_credentials` (None)
- **GDBusProxy** — `new` + `get_cached_property_names` (empty), `set_cached_property` + `get_cached_property` (match)
- **GDBusInterface** — `SimpleDBusInterface::new` + `get_info` (match), `set_object`/`get_object` (match)
- **GDBusObject** — `SimpleDBusObject::new` + `get_object_path` (match), `add_interface` + `get_interface` (match)
- **GDBusObjectSkeleton** — `new` + `set_object_path` (match), `add_interface` + `interface_count` (1)
- **GDBusObjectManager** — `new` + `add_object` + `object_count` (1), `get_interface` (true)
- **GDBusMenuModel** — `new` + `add_item` + `get_n_items` (1), `get_item` (match)
- **GDBusNameOwning** — `own_name` + `get_name_state` (Owned), `unown_name` (true)
- **GDBusNameWatching** — `watch_name` + `name_appeared` + `get_name_owner` (match)
- **GDBusUtils** — `is_name` (true), `is_unique_name` (true), `is_interface_name` (true), `escape_object_path` (match)
- **GDBusAuthObserver** — `allow_mechanism` + `get_allowed_mechanisms` (1), `authorize_authenticated_peer` + `is_peer_authorized` (true)
- **GSimpleAsyncResult** — `new` + `is_complete` (false), `complete` + `is_complete` (true), `set_error` + `had_error` (true)
- **GTlsPassword** — `new_with` + `get_description` (match), `set_password` + `get_password` (match)
- **GRemoteActionGroup** — `add_action` + `activate_action` (true), `change_action_state` + `get_action` (match)
- **GDBusActionGroup** — `new` + `get_bus_name` (match), `add_action` + `action_count` (1)
- **GActionGroupExporter** — `export` + `export_count` (1), `get_exported_actions` (match)
- **GApplicationCommandLine** — `new` + `get_arguments` (match), `print` + `get_stdout_data` (match)
- **GDesktopAppInfo** — `new` + `set_display_name` + `get_display_name` (match), `add_category` + `get_categories` (1)
- **GSimpleProxyResolver** — `new` + `lookup` (default proxy), `set_proxy` + `lookup` (per-scheme)
- **GDummyProxyResolver** — `lookup` (direct://), `is_supported` (true)
- **GSocketInputStream** — `inject` + `read` (match), `close` + `is_closed` (true)
- **GSocketOutputStream** — `write` + `get_tx_data` (match), `close` + `is_closed` (true)
- **GNativeSocketAddress** — `new` + `get_family` (match), `get_native` (match)
- **GThreadedResolver** — `add_host` + `lookup_by_name` (match), `lookup_by_address` (match)
- **GIOModule** — `new` + `get_path` (match), `load` + `is_loaded` (true)
- **GIOScheduler** — `push_job` + `job_count` (1), `pop_job` (highest priority first)
- **GUnixInputStream** — `new` + `get_fd` (match), `inject` + `read` (match)
- **GUnixOutputStream** — `new` + `get_fd` (match), `write` + `get_tx_data` (match)
- **GUnixFDList** — `append` + `get_length` (2), `peek_fds` (match)
- **GUnixFDMessage** — `append_fd` + `get_fd_count` (1), `get_fd_list` (match)
- **GUnixCredentialsMessage** — `new` + `get_credentials` (ok), `new_with_credentials` (pid ok)
- **GDelayedSettingsBackend** — `write` + `get_has_unapplied` (true), `apply` + `read` (match)
- **GMemoryMonitor** — `get_level` (low), `set_level` + `get_level` (match)
- **GDBusInterfaceSkeleton** — `new` + `export` + `is_exported` (true), `unexport` + `is_exported` (false)
- **GDBusObjectProxy** — `new` + `get_bus_name` (match), `add_interface` + `interface_count` (1)
- **GDBusObjectManagerClient** — `new` + `add_proxy` + `proxy_count` (1), `remove_proxy` (true)
- **GDBusObjectManagerServer** — `new` + `export` + `is_exported` (true), `add_object` + `object_count` (1)
- **GDBusAddress** — `parse_entry` (unix) + `is_unix` (true), `dbus_address_escape_value`, `is_supported_address` (loopback), `dbus_address_get_for_bus_sync` (None→loopback:), `register_dbus_address_platform`
- **GProxyAddressEnumerator** — `new` + `next` (direct/http), `new_with_lookup` (SimpleProxyResolver/DummyProxyResolver), `reset`
- **GDir platform** — `register_dir_platform` + `dir_open` (mock entries via platform hook)
- **GSpawn platform** — `register_spawn_platform` + `spawn_async`/`spawn_sync`; RustOS fork/exec via `process_manager` + ELF loader (`src/glib_spawn.rs`), `envp` on child stack, `spawn_sync` IPC pipe capture for stdout/stderr, user-mode execution via `src/user_sched.rs` (`yield_cpu`, timer tick, desktop idle), children registered on the kernel scheduler via `adopt_spawned_process`
- **GMappedFile platform** — `register_mapped_file_platform` + `mapped_file_new`/`mapped_file_new_from_fd`
- **GMenuExporter** — `export` + `export_count` (1), `unexport` (true)
- **GDebugController** — `new` + `get_debug_enabled` (false), `set_debug_enabled` (true)
- **GNotificationBackend** — `send_notification` + `pending_count` (1), `withdraw_notification` (true)
- **GResourceFile** — `new` + `get_uri` (resource://), `new_with_contents` + `size` (match)
- **GHttpProxy** — `new` + `get_uri` (match), `connect` + `is_connected` (true)
- **GSocks4AProxy** — `new` + `connect` + `is_connected` (true), `supports_hostname` (true)
- **GSocks4Proxy** — `new` + `connect` + `is_connected` (true), `supports_hostname` (false)
- **GSocks5Proxy** — `new` + `connect` + `authenticate` + `is_authenticated` (true)
- **GNetworkMonitorBase** — `new` + `get_network_available` (false), `set_network_available` + `can_reach` (true)
- **GPollFileMonitor** — `new` + `poll` (true), `cancel` + `is_cancelled` (true)
- **GPortalSupport** — `new` + `is_available` (false), `set_available` + `set_desktop` (match)
- **GOpenURIPortal** — `new` + `open_uri` + `get_opened_uris` (1)
- **GAsyncHelper** — `queue` + `pending_count` (1), `drain` (1), `cancel` (true)
- **GContextSpecificGroup** — `add` + `count` (2), `remove` (true), `get` (match)
- **GPollableUtils** — `is_readable` (IN, true), `is_writable` (OUT, true), `is_closed` (ERR, true)
- **GDBusAuth** — `new` + `get_state` (WaitingForBegin), `authenticate` + `is_authenticated` (true)
- **GDBusAuthMechanism** — `SimpleAuthMechanism::new` + `name` (match), `process_data` (Accepted)
- **GDBusAuthMechanismAnon** — `new` + `name` (ANONYMOUS), `initiate` (some), `process_data` (Accepted)
- **GDBusAuthMechanismExternal** — `new` + `process_data` (match→Accepted, mismatch→Rejected)
- **GDBusAuthMechanismSha1** — `new` + `name` (DBUS_COOKIE_SHA1), `process_data` (match→Accepted)
- **GDebugControllerDBus** — `new` + `get_object_path` (match), `export` + `is_exported` (true)
- **GTestDBus** — `new` + `start` + `is_running` (true), `set_bus_address` + `get_bus_address` (match)
- **GSandbox** — `new` + `is_sandboxed` (false), `set_type` (Flatpak) + `is_sandboxed` (true)
- **GDummyFile** — `new` + `get_uri` (match), `get_basename` (match), `get_path` (file://→match)
- **GIPTosMessage** — `new` + `get_tos` (match), `get_size` (1)
- **GIPv6TClassMessage** — `new` + `get_tclass` (match), `get_level` (41), `get_type` (67)
- **GNativeVolumeMonitor** — `add_volume` + `volume_count` (2), `add_mount` + `mount_count` (1)
- **GUnionVolumeMonitor** — `add_monitor` + `volume_count` (aggregate), `get_all_volumes` (match)
- **GUnixMountEntry** — `new` + `get_mount_path` (match), `get_filesystem_type` (match)
- **GUnixMounts** — `add` + `get_mounts` (1), `has_changed` (true→false), `find_by_path` (true)
- **GUnixVolume** — `new` + `mount` + `is_mounted` (true), `unmount` + `is_mounted` (false)
- **GUnixVolumeMonitor** — `add_volume` + `volume_count` (2), `mounted_count` (1)
- **GRegistrySettingsBackend** — `write` + `read` (match), `reset` (true)
- **GLocalFile** — `new` + `get_uri` (file://), `create` + `query_exists` (true), `delete` (false)
- **GLocalFileEnumerator** — `add_child` + `next` (match), `next` (none after drain)
- **GLocalFileInfo** — `new` + `get_file_type` (Unknown), `set_file_type` (Regular) + `is_hidden` (true)
- **GLocalFileInputStream** — `new` + `read` (5 bytes), `close` + `read` (0)
- **GLocalFileOutputStream** — `new` + `write` (5 bytes), `close` + `write` (0)
- **GLocalFileIOStream** — `new` + `read` (3) + `write` (9), `get_data` (match)
- **GLocalFileMonitor** — `notify_changed` + `has_changed` (true→false)
- **GLocalVfs** — `get_file_for_uri` (file://→path), `is_local` (true/false)
- **GMemoryMonitorDBus** — `new` + `connect` + `is_connected` (true), `set_level` (Critical)
- **GMemoryMonitorPoll** — `set_level` (Low) + `poll` (match) + `poll_count` (1)
- **GMemoryMonitorPortal** — `set_available` (true) + `set_level` (Critical) + `get_level` (match)
- **GMemoryMonitorPsi** — `update_from_psi` (5,0→Normal, 60,0→Low, 60,55→Critical)
- **GPowerProfileMonitorDBus** — `set_power_saver_enabled` (true) + `get_profile` (PowerSaver)
- **GPowerProfileMonitorPortal** — `set_available` (true) + `set_power_saver_enabled` (true)
- **GProxyResolverPortal** — `lookup` unavailable (direct://), `set_proxies` + `lookup` (match)
- **GTrashPortal** — `trash_file` unavailable (false), `set_available` + `trash_file` (true)
- **GDocumentPortal** — `set_available` + `add_document` (true), `get_document_path` (match)
- **GNetworkMonitorNetlink** — `add_link` + `add_route` + `set_available` + `is_network_available` (true)
- **GNetworkMonitorNM** — `set_connectivity` (Full) + `is_network_available` (true)
- **GNetworkMonitorPortal** — `set_available` + `set_network_available` (true)
- **GApplicationImpl** — `new` + `get_app_id` (match), `register` + `is_registered` (true)
- **GDBusDaemon** — `start` + `register_name` (true) + `lookup_name` (match)
- **GIoEnums** — `FileMonitorEvent::Created` (eq), `FileCopyFlags::OVERWRITE` (1)
- **GThumbnailVerify** — `thumbnail_verify` empty (NotFound), `get_thumbnail_path` (png), `is_thumbnail_path` (true)

### Private internals (complete)

Two groups of internal modules are declared in `lib.rs`:

**Implementation helpers** (`// Internal/private modules`):

| Module | C source | Notes |
|--------|----------|-------|
| `strinfo` | `glib/strinfo.c` | UTF-8 character property lookup tables |
| `gsettings_mapping` | `gio/gsettings-mapping.c` | `GValue` ↔ `GVariant` mapping for GSettings |
| `giomodule_priv` | `gio/giomodule-priv.c` | IoModule registration helpers |
| `giounix_private` | `gio/giounix-private.c` | Unix-specific GIO utilities |
| `giowin32_private` | `gio/giowin32-private.c` | UTF-16 path helpers, `wcsicmp`, basename |
| `gdbusprivate` | `gio/gdbusprivate.c` | D-Bus shared private helpers |
| `gcontenttype_fdo` | `gio/gcontenttype-fdo.c` | Freedesktop MIME database backend stub |
| `gcontenttype_win32` | `gio/gcontenttype-win32.c` | Windows MIME/registry backend stub |
| `gmarshal_internal` | `gobject/gmarshal-internal.c` | GObject marshaller tables (stub) |
| `gapplicationimpl_dbus` | `gio/gapplicationimpl-dbus.c` | D-Bus `GApplication` backend hooks |
| `gmemorymonitorwin32` | `gio/gmemorymonitorwin32.c` | Win32 memory-pressure monitor stub |

**Private headers** (`// Private header modules` — mirrors `*-private.h` / `*-priv.h`):

| Module | C source | Notes |
|--------|----------|-------|
| `gappinfoprivate` | `gio/gappinfoprivate.c` | App-info launch/URI helpers |
| `gcontenttypeprivate` | `gio/gcontenttypeprivate.c` | MIME type sniffing hooks |
| `gcredentialsprivate` | `gio/gcredentialsprivate.c` | Credentials message helpers |
| `gio_trace` | `gio/gio_trace.c` | GIO trace points (stub) |
| `gioprivate` | `gio/gioprivate.c` | Core GIO private helpers |
| `gmountprivate` | `gio/gmountprivate.c` | Mount private API |
| `gnetworkingprivate` | `gio/gnetworkingprivate.c` | Socket creation helpers |
| `gnotification_private` | `gio/gnotification-private.c` | Notification getters/serialization |
| `gosxappinfo` | `gio/gosxappinfo.c` | macOS app-info backend stub |
| `gsettingsbackendinternal` | `gio/gsettingsbackendinternal.c` | Settings backend internals |
| `gsettingsschema_internal` | `gio/gsettingsschema-internal.c` | Schema tree/key helpers |
| `gsubprocesslauncher_private` | `gio/gsubprocesslauncher-private.c` | Subprocess launcher state |
| `gthreadedresolver_private` | `gio/gthreadedresolver-private.c` | Resolver worker helpers |
| `gunixmounts_private` | `gio/gunixmounts-private.c` | Unix mount table helpers |
| `gdbusactiongroup_private` | `gio/gdbusactiongroup-private.c` | D-Bus action-group export |
| `gfileattribute_priv` | `gio/gfileattribute-priv.c` | Attribute value storage helpers |
| `gfileinfo_priv` | `gio/gfileinfo-priv.c` | Attribute ID constants, set-by-id |
| `giowin32_afunix` | `gio/giowin32-afunix.c` | Win32 AF_UNIX socket stub |
| `giowin32_priv` | `gio/giowin32-priv.c` | Win32 GIO private helpers |

Host unit tests: **2822** tests in `glib-native` (`make test-glib-native`).

### C ABI packaging (Phase 13)

Build a host static library and C header for linking remaining C code against Rust GLib:

```bash
# From repo root
make build-glib-static

# Or from glib workspace
cd glib-rust/glib/rust && make staticlib   # libglib_native.a
make header                              # regenerate include/glib_native.h
make c-ffi-smoke                         # link/run examples/c_ffi_smoke.c
```

- **`glib-native/Cargo.toml`** — `crate-type = ["rlib", "staticlib"]`, `c-abi` feature (enables `std` for host staticlib link)
- **`include/glib_native.h`** — cbindgen output from `ffi.rs` (70+ symbols)
- **`ffi_parity.rs`** — systematic `extern "C"` parity tests (memory, quark, type, value, object, error, signal)
- **`examples/c_ffi_smoke.c`** — minimal `g_type_init` / `g_malloc` / `g_free` link test

### GObject Introspection (complete)

All 35 `.c`/`.h` files in `girepository/` have been ported to Rust modules.

| Module | Notes |
|--------|-------|
| `gitypelib` | Typelib header parse, in-memory typelib builder |
| `gibaseinfo` | `InfoType`, `BaseInfo` ref-counted via `Arc` |
| `gitypeinfo` | `TypeTag` enum, pointer/param type accessors |
| `gienuminfo` | `EnumInfo`, `ValueInfo` |
| `girepository` | Search paths, `require`/`find_by_name`, in-memory registry |
| `gitypes` | `GIArgument`, `GITransfer`, `GIDirection`, `GITypeTag`, `GIArrayType`, flags structs |
| `giarginfo` | `ArgInfo` with direction, scope, ownership transfer |
| `gicallableinfo` | `CallableInfo` base for functions/callbacks/signals/vfuncs |
| `gicallbackinfo` | `CallbackInfo` extending `CallableInfo` |
| `giconstantinfo` | `ConstantInfo` with `GIArgument` value |
| `gifieldinfo` | `FieldInfo` with flags, size, offset |
| `giflagsinfo` | `FlagsInfo` extending `EnumInfo` |
| `gifunctioninfo` | `FunctionInfo` with symbol, flags, `InvokeError` |
| `giinterfaceinfo` | `InterfaceInfo` with prerequisites, properties, methods, signals, vfuncs |
| `giobjectinfo` | `ObjectInfo` with parent, interfaces, fields, properties, methods |
| `gipropertyinfo` | `PropertyInfo` with flags, setter/getter |
| `giregisteredtypeinfo` | `RegisteredTypeInfo` with type name, GType |
| `gisignalinfo` | `SignalInfo` with `SignalFlags`, class closure |
| `gistructinfo` | `StructInfo` with fields, methods, size, alignment |
| `giunioninfo` | `UnionInfo` with discriminators |
| `giunresolvedinfo` | `UnresolvedInfo` placeholder |
| `givalueinfo` | `ValueInfo` re-export from `gienuminfo` |
| `givfuncinfo` | `VFuncInfo` with flags, offset, signal, invoker |
| `girffi` | FFI integration (stubbed — libffi not available) |
| `girnode` | GIR node tree with `NodeTag`, `Node` |
| `girparser` | GIR XML parser (stubbed) |
| `girwriter` | GIR XML writer with node serialization |
| `girmodule` | GIR module with namespace, entries, dependencies |
| `ginvoke` | Function invocation (stubbed) |
| `gdump` | Type dumper (stubbed) |
| `giroffsets` | Struct/union offset computation with `align_to` |
| `gthash` | String hash table for typelib deduplication |
| `gi_dump_types` | Type dump utility (stubbed) |
| `girepository_autocleanups` | No-op (Rust `Drop` replaces C autoptr) |
| `gibaseinfo_private` | Private `BaseInfo` constructor |
| `girepository_private` | Private repository state |
| `girmodule_private` | Private module setters |
| `girnode_private` | Private node helpers |
| `girparser_private` | Private parser state |
| `girwriter_private` | Private writer state |
| `gitypelib_internal` | Typelib binary header structures |

### Win32 platform modules (complete)

Windows-specific GIO ports live under `// Win32 platform modules`. Each module uses
in-memory stubs instead of real Win32/COM calls, matching the pattern established by
`gwin32inputstream` and `gmemorymonitorwin32`:

| Module | C source | Key types |
|--------|----------|-----------|
| `gwin32inputstream` | `gwin32inputstream.c` | `Win32InputStream` |
| `gwin32outputstream` | `gwin32outputstream.c` | `Win32OutputStream` |
| `gwin32mount` | `gwin32mount.c` | `Win32Mount` |
| `gwin32volumemonitor` | `gwin32volumemonitor.c` | `Win32VolumeMonitor` (drive bitmask A:–Z:) |
| `gwin32networkmonitor` | `gwin32networkmonitor.c` | `Win32NetworkMonitor` (route table stub) |
| `gwin32notificationbackend` | `gwin32notificationbackend.c` | `Win32NotificationBackend` (Shell_NotifyIcon stub) |
| `gwin32file_sync_stream` | `gwin32file-sync-stream.c` | `Win32FileSyncStream` (COM `IStream` stub) |
| `gwin32packageparser` | `gwin32packageparser.c` | `PackageParser` (AppxManifest XML) |
| `gwin32appinfo` | `gwin32appinfo.c` | `Win32AppInfo`, `Win32AppInfoRegistry`, verb/UWP support |
| `gwin32sid` | `gwin32sid.c` | `Win32Sid` |
| `gwin32registrykey` | `gwin32registrykey.c` | `Win32RegistryKey` |

### macOS platform module (complete)

| Module | C source | Key types |
|--------|----------|-----------|
| `gosxnetworkmonitor` | `gosxnetworkmonitor.c` | `OsxNetworkMonitor` — simulated PF_ROUTE socket + sysctl routing table; re-exports `NetworkConnectivity` from `gnetworkmonitor` |

### CLI tools (complete)

All GIO/GLib command-line utilities are ported as library modules with
`pub fn run(args: &[&str]) -> i32` (0 = success). They use an in-memory VFS via
`gio_tool::ToolFilePlatform` rather than calling `std::process` or real OS I/O.

**Shared helpers (`gio_tool.rs`):** `print_error`, `print_file_error`, `file_type_to_string`,
`attribute_type_to_string`, `show_help`, in-memory stdout/stderr capture.

**`gio` subcommands (17):** `gio_tool_cat`, `_copy`, `_info`, `_launch`, `_list`, `_mime`,
`_mkdir`, `_monitor`, `_mount`, `_move`, `_open`, `_remove`, `_rename`, `_save`, `_set`,
`_trash`, `_tree`.

**Standalone tools (8):** `gapplication_tool`, `gsettings_tool`, `gdbus_tool`,
`gresource_tool`, `glib_compile_resources`, `glib_compile_schemas`, `gio_launch_desktop`,
`gio_querymodules`.

Compile/schema tools also expose pure functions (`compile_resources`, `compile_schemas`,
`query_modules`) for use without a CLI front-end.

## Running Rust tests

### glib-native (host `std`)

The repo-root `.cargo/config.toml` sets the default target to `x86_64-rustos.json` for
kernel work. **Do not** enable global `build-std` there — it breaks host tests by
linking two copies of `alloc` when `#[cfg(test)]` pulls in `std`.

Run glib-native tests on the host toolchain:

```bash
# From repo root
make test-glib-native
make check-glib-native

# Or from the glib workspace
cd glib-rust/glib/rust && make test
cd glib-rust/glib/rust && ./scripts/test.sh
```

Override the host triple if needed:

```bash
HOST_TARGET=x86_64-unknown-linux-gnu make test-glib-native
```

### RustOS kernel (`no_std` + `build-std`)

Kernel builds and tests pass `-Zbuild-std` explicitly via `build_rustos.sh`:

```bash
make build          # kernel binary
make test           # kernel tests (custom target + build-std)
./build_rustos.sh --check-only
```

## Contributing

When adding a new converted module:

1. Add the module under `rust/glib-native/src/`.
2. Port or mirror relevant tests from `glib/tests/`.
3. Update the phase table in this document.
4. Do not remove C sources until the phase’s FFI integration is complete and
   upstream tests pass with the Rust backend.
