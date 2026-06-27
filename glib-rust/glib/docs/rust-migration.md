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
| **11** | GIO (split into sub-phases: streams, sockets, D-Bus, settings, …) | `gio/*` | **Partial** (gfileattribute ported: `FileAttributeType` enum (10 types), `FileAttributeInfoFlags` (COPY_WITH_FILE/COPY_WHEN_MOVED), `FileAttributeInfo` struct, `FileAttributeInfoList` ref-counted sorted-by-name list with binary-search `lookup`/`add`/`dup`/`ref_`/`n_infos`/`infos`, 14 unit tests passing; gdbusintrospection ported: 7 ref-counted info structs (Annotation/Arg/Method/Signal/Property/Interface/Node) with `Arc<T>` ref counting, `DBusPropertyInfoFlags` (READABLE/WRITABLE), lookup helpers (`dbus_annotation_info_lookup`/`dbus_interface_info_lookup_method`/`_signal`/`_property`/`dbus_node_info_lookup_interface`), 12 unit tests passing; remaining GIO submodules — streams, sockets, D-Bus connection, settings, GFile, GIcon, GCancellable, GAsyncResult, etc. — planned; XML parse/generate for introspection info deferred) |
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
  pick leaf boxed types (`GFileAttributeInfoList` and the
  `GDBus*Info` structs are all `G_DEFINE_BOXED_TYPE`) that don't
  depend on the interface system.
- `g_dbus_node_info_new_for_xml` (XML parsing of introspection data)
  and `g_dbus_interface_info_generate_xml` / `_node_info_generate_xml`
  (XML generation) — deferred; need GMarkup parser integration and
  the introspection parser state machine.
- `g_dbus_interface_info_cache_build` / `_release` (per-interface
  name→info lookup cache) — deferred; needs a global cache table.

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

**54 modules** ported across Phases 1–11, all wired into RustOS:

| Phase | Modules | Status |
|-------|---------|--------|
| 1 | endian, checked, refcount, bytes, refstring, rcbox | Done (6) |
| 2 | atomic, mem, strfuncs, gstring, rand, printf, slice | Done (7) |
| 3 | array, list, queue, ptr_array, node, sequence, completion, qsort, primes | Done (9) |
| 4 | hash, tree, dataset, quark, relation, cache | Done (6) |
| 5 | error, messages, option | Done (3) |
| 6 | fileutils, convert, charset, checksum, base64, hmac, hostutils, environ, keyfile, bitlock, hook, pattern, shell, uri, markup, stringchunk, strvbuilder, version, scanner, timer, utils, pathbuf, uuid, regex, testutils + dir/mappedfile/spawn/stdio (stubs) | Partial (29) |
| 7 | date, datetime, timezone, varianttype, variant, unicode, utf8 | Partial (7) |
| 8 | asyncqueue, thread, poll, iochannel, mainloop, threadpool | Partial (6) |
| 9 | gtype, gvalue, gparamspec, gsignal, gobject | Partial (5) |
| 10 | gmodule | Partial (1) |
| 11 | gfileattribute, gdbusintrospection | Partial (2) |

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
