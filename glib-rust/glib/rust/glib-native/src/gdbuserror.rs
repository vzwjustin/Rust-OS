//! GIO D-Bus error handling matching `gio/gdbuserror.h` /
//! `gio/gdbuserror.c`.
//!
//! Provides:
//! - `DBusError` enum (44 well-known `org.freedesktop.DBus.Error.*`
//!   codes).
//! - `DBusErrorEntry` struct for registering error-domain tables.
//! - `dbus_error_quark()` — the `G_DBUS_ERROR` quark, lazily
//!   registered with all 44 well-known entries.
//! - `dbus_error_register_error` / `dbus_error_unregister_error` /
//!   `dbus_error_register_error_domain` — global registry mapping
//!   `(Quark, code)` <-> `dbus_error_name`.
//! - `dbus_error_is_remote_error` / `dbus_error_get_remote_error` /
//!   `dbus_error_strip_remote_error` — recognize and handle the
//!   `"GDBus.Error:NAME: "` prefix that GIO adds to remote errors.
//! - `dbus_error_new_for_dbus_error` — build a `glib_native::Error`
//!   from a D-Bus error name + message.
//! - `dbus_error_encode_gerror` — encode an `Error` back into a D-Bus
//!   error name string (with the `org.gtk.GDBus.UnmappedGError.Quark._`
//!   fallback for unregistered domains).
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::error::Error;
use crate::prelude::*;
use crate::quark::{quark_from_static_string, quark_from_string, Quark};
use crate::strfuncs::str_has_prefix;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::mutex::Mutex;
use spin::once::Once;

// ─────────────────────────── GDBusError enum ──────────────────────────────

/// Well-known D-Bus error codes (`GDBusError`).
///
/// Matches the upstream enum order so discriminant values are stable
/// across the C and Rust implementations. Each variant corresponds to
/// an `org.freedesktop.DBus.Error.*` name (see `WELL_KNOWN_ENTRIES`
/// below).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum DBusError {
    /// `org.freedesktop.DBus.Error.Failed` — generic error.
    Failed = 0,
    /// `org.freedesktop.DBus.Error.NoMemory`.
    NoMemory = 1,
    /// `org.freedesktop.DBus.Error.ServiceUnknown`.
    ServiceUnknown = 2,
    /// `org.freedesktop.DBus.Error.NameHasNoOwner`.
    NameHasNoOwner = 3,
    /// `org.freedesktop.DBus.Error.NoReply`.
    NoReply = 4,
    /// `org.freedesktop.DBus.Error.IOError`.
    IoError = 5,
    /// `org.freedesktop.DBus.Error.BadAddress`.
    BadAddress = 6,
    /// `org.freedesktop.DBus.Error.NotSupported`.
    NotSupported = 7,
    /// `org.freedesktop.DBus.Error.LimitsExceeded`.
    LimitsExceeded = 8,
    /// `org.freedesktop.DBus.Error.AccessDenied`.
    AccessDenied = 9,
    /// `org.freedesktop.DBus.Error.AuthFailed`.
    AuthFailed = 10,
    /// `org.freedesktop.DBus.Error.NoServer`.
    NoServer = 11,
    /// `org.freedesktop.DBus.Error.Timeout`.
    Timeout = 12,
    /// `org.freedesktop.DBus.Error.NoNetwork`.
    NoNetwork = 13,
    /// `org.freedesktop.DBus.Error.AddressInUse`.
    AddressInUse = 14,
    /// `org.freedesktop.DBus.Error.Disconnected`.
    Disconnected = 15,
    /// `org.freedesktop.DBus.Error.InvalidArgs`.
    InvalidArgs = 16,
    /// `org.freedesktop.DBus.Error.FileNotFound`.
    FileNotFound = 17,
    /// `org.freedesktop.DBus.Error.FileExists`.
    FileExists = 18,
    /// `org.freedesktop.DBus.Error.UnknownMethod`.
    UnknownMethod = 19,
    /// `org.freedesktop.DBus.Error.TimedOut`.
    TimedOut = 20,
    /// `org.freedesktop.DBus.Error.MatchRuleNotFound`.
    MatchRuleNotFound = 21,
    /// `org.freedesktop.DBus.Error.MatchRuleInvalid`.
    MatchRuleInvalid = 22,
    /// `org.freedesktop.DBus.Error.Spawn.ExecFailed`.
    SpawnExecFailed = 23,
    /// `org.freedesktop.DBus.Error.Spawn.ForkFailed`.
    SpawnForkFailed = 24,
    /// `org.freedesktop.DBus.Error.Spawn.ChildExited`.
    SpawnChildExited = 25,
    /// `org.freedesktop.DBus.Error.Spawn.ChildSignaled`.
    SpawnChildSignaled = 26,
    /// `org.freedesktop.DBus.Error.Spawn.Failed`.
    SpawnFailed = 27,
    /// `org.freedesktop.DBus.Error.Spawn.FailedToSetup`.
    SpawnSetupFailed = 28,
    /// `org.freedesktop.DBus.Error.Spawn.ConfigInvalid`.
    SpawnConfigInvalid = 29,
    /// `org.freedesktop.DBus.Error.Spawn.ServiceNotValid`.
    SpawnServiceInvalid = 30,
    /// `org.freedesktop.DBus.Error.Spawn.ServiceNotFound`.
    SpawnServiceNotFound = 31,
    /// `org.freedesktop.DBus.Error.Spawn.PermissionsInvalid`.
    SpawnPermissionsInvalid = 32,
    /// `org.freedesktop.DBus.Error.Spawn.FileInvalid`.
    SpawnFileInvalid = 33,
    /// `org.freedesktop.DBus.Error.Spawn.NoMemory`.
    SpawnNoMemory = 34,
    /// `org.freedesktop.DBus.Error.UnixProcessIdUnknown`.
    UnixProcessIdUnknown = 35,
    /// `org.freedesktop.DBus.Error.InvalidSignature`.
    InvalidSignature = 36,
    /// `org.freedesktop.DBus.Error.InvalidFileContent`.
    InvalidFileContent = 37,
    /// `org.freedesktop.DBus.Error.SELinuxSecurityContextUnknown`.
    SelinuxSecurityContextUnknown = 38,
    /// `org.freedesktop.DBus.Error.AdtAuditDataUnknown`.
    AdtAuditDataUnknown = 39,
    /// `org.freedesktop.DBus.Error.ObjectPathInUse`.
    ObjectPathInUse = 40,
    /// `org.freedesktop.DBus.Error.UnknownObject`.
    UnknownObject = 41,
    /// `org.freedesktop.DBus.Error.UnknownInterface`.
    UnknownInterface = 42,
    /// `org.freedesktop.DBus.Error.UnknownProperty`.
    UnknownProperty = 43,
    /// `org.freedesktop.DBus.Error.PropertyReadOnly`.
    PropertyReadOnly = 44,
}

impl DBusError {
    /// Numeric error code matching the upstream enum discriminant.
    pub fn to_code(self) -> i32 {
        self as i32
    }

    /// The `org.freedesktop.DBus.Error.*` name for this code.
    pub fn to_dbus_name(self) -> &'static str {
        WELL_KNOWN_ENTRIES
            .iter()
            .find(|(code, _)| *code == self as i32)
            .map(|(_, name)| *name)
            .expect("WELL_KNOWN_ENTRIES covers all DBusError variants")
    }
}

// ────────────────────────── GDBusErrorEntry ───────────────────────────────

/// Struct used in `dbus_error_register_error_domain`
/// (`GDBusErrorEntry`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DBusErrorEntry {
    /// Error code within the domain.
    pub error_code: i32,
    /// D-Bus error name, e.g. `"org.freedesktop.DBus.Error.Failed"`.
    pub dbus_error_name: &'static str,
}

/// Well-known `(code, dbus_name)` entries for the `G_DBUS_ERROR` quark.
/// Mirrors the `g_dbus_error_entries` table in `gdbuserror.c`.
const WELL_KNOWN_ENTRIES: &[(i32, &str)] = &[
    (0, "org.freedesktop.DBus.Error.Failed"),
    (1, "org.freedesktop.DBus.Error.NoMemory"),
    (2, "org.freedesktop.DBus.Error.ServiceUnknown"),
    (3, "org.freedesktop.DBus.Error.NameHasNoOwner"),
    (4, "org.freedesktop.DBus.Error.NoReply"),
    (5, "org.freedesktop.DBus.Error.IOError"),
    (6, "org.freedesktop.DBus.Error.BadAddress"),
    (7, "org.freedesktop.DBus.Error.NotSupported"),
    (8, "org.freedesktop.DBus.Error.LimitsExceeded"),
    (9, "org.freedesktop.DBus.Error.AccessDenied"),
    (10, "org.freedesktop.DBus.Error.AuthFailed"),
    (11, "org.freedesktop.DBus.Error.NoServer"),
    (12, "org.freedesktop.DBus.Error.Timeout"),
    (13, "org.freedesktop.DBus.Error.NoNetwork"),
    (14, "org.freedesktop.DBus.Error.AddressInUse"),
    (15, "org.freedesktop.DBus.Error.Disconnected"),
    (16, "org.freedesktop.DBus.Error.InvalidArgs"),
    (17, "org.freedesktop.DBus.Error.FileNotFound"),
    (18, "org.freedesktop.DBus.Error.FileExists"),
    (19, "org.freedesktop.DBus.Error.UnknownMethod"),
    (20, "org.freedesktop.DBus.Error.TimedOut"),
    (21, "org.freedesktop.DBus.Error.MatchRuleNotFound"),
    (22, "org.freedesktop.DBus.Error.MatchRuleInvalid"),
    (23, "org.freedesktop.DBus.Error.Spawn.ExecFailed"),
    (24, "org.freedesktop.DBus.Error.Spawn.ForkFailed"),
    (25, "org.freedesktop.DBus.Error.Spawn.ChildExited"),
    (26, "org.freedesktop.DBus.Error.Spawn.ChildSignaled"),
    (27, "org.freedesktop.DBus.Error.Spawn.Failed"),
    (28, "org.freedesktop.DBus.Error.Spawn.FailedToSetup"),
    (29, "org.freedesktop.DBus.Error.Spawn.ConfigInvalid"),
    (30, "org.freedesktop.DBus.Error.Spawn.ServiceNotValid"),
    (31, "org.freedesktop.DBus.Error.Spawn.ServiceNotFound"),
    (32, "org.freedesktop.DBus.Error.Spawn.PermissionsInvalid"),
    (33, "org.freedesktop.DBus.Error.Spawn.FileInvalid"),
    (34, "org.freedesktop.DBus.Error.Spawn.NoMemory"),
    (35, "org.freedesktop.DBus.Error.UnixProcessIdUnknown"),
    (36, "org.freedesktop.DBus.Error.InvalidSignature"),
    (37, "org.freedesktop.DBus.Error.InvalidFileContent"),
    (
        38,
        "org.freedesktop.DBus.Error.SELinuxSecurityContextUnknown",
    ),
    (39, "org.freedesktop.DBus.Error.AdtAuditDataUnknown"),
    (40, "org.freedesktop.DBus.Error.ObjectPathInUse"),
    (41, "org.freedesktop.DBus.Error.UnknownObject"),
    (42, "org.freedesktop.DBus.Error.UnknownInterface"),
    (43, "org.freedesktop.DBus.Error.UnknownProperty"),
    (44, "org.freedesktop.DBus.Error.PropertyReadOnly"),
];

// ──────────────────────────── registry ────────────────────────────────────

/// Internal registered-error record. Mirrors `RegisteredError` in
/// `gdbuserror.c`. Shared via `Arc` so both indexes can point to the
/// same record without double-allocating.
#[derive(Debug, Clone)]
struct RegisteredError {
    error_domain: Quark,
    error_code: i32,
    dbus_error_name: String,
}

/// Global registry. Mirrors the two hash tables in `gdbuserror.c`:
/// `quark_code_pair_to_re` and `dbus_error_name_to_re`. We use
/// `BTreeMap` (no_std friendly) instead of `HashMap`.
struct Registry {
    /// `(domain, code)` → `RegisteredError`.
    by_pair: BTreeMap<(Quark, i32), Arc<RegisteredError>>,
    /// `dbus_error_name` → `RegisteredError`.
    by_name: BTreeMap<String, Arc<RegisteredError>>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    by_pair: BTreeMap::new(),
    by_name: BTreeMap::new(),
});

/// Lazily-initialized `G_DBUS_ERROR` quark. Registered with all 44
/// well-known entries on first access.
static DBUS_ERROR_QUARK: Once<Quark> = Once::new();

/// Quark for the GDBus error domain (`g_dbus_error_quark`).
///
/// On first call, registers all 44 well-known
/// `org.freedesktop.DBus.Error.*` entries via
/// `dbus_error_register_error_domain`.
pub fn dbus_error_quark() -> Quark {
    *DBUS_ERROR_QUARK.call_once(|| {
        let quark = quark_from_static_string(Some("g-dbus-error-quark"));
        // Register every well-known entry. Build a static-friendly
        // slice of DBusErrorEntry and call the domain registrar.
        let entries: Vec<DBusErrorEntry> = WELL_KNOWN_ENTRIES
            .iter()
            .map(|&(code, name)| DBusErrorEntry {
                error_code: code,
                dbus_error_name: name,
            })
            .collect();
        dbus_error_register_error_domain("g-dbus-error-quark", quark, &entries);
        quark
    })
}

/// Register a single `(domain, code) <-> dbus_error_name` association
/// (`g_dbus_error_register_error`).
///
/// Returns `true` if the association was created, `false` if either the
/// `(domain, code)` pair or the `dbus_error_name` was already
/// registered (with a different counterpart).
pub fn dbus_error_register_error(
    error_domain: Quark,
    error_code: i32,
    dbus_error_name: &str,
) -> bool {
    let mut reg = REGISTRY.lock();
    // If the name is already registered, don't overwrite.
    if reg.by_name.contains_key(dbus_error_name) {
        return false;
    }
    // If the (domain, code) pair is already registered, don't overwrite.
    if reg.by_pair.contains_key(&(error_domain, error_code)) {
        return false;
    }
    let re = Arc::new(RegisteredError {
        error_domain,
        error_code,
        dbus_error_name: dbus_error_name.to_owned(),
    });
    reg.by_pair
        .insert((error_domain, error_code), Arc::clone(&re));
    reg.by_name.insert(dbus_error_name.to_owned(), re);
    true
}

/// Unregister a `(domain, code) <-> dbus_error_name` association
/// (`g_dbus_error_unregister_error`).
///
/// Returns `true` if the association existed and was removed.
pub fn dbus_error_unregister_error(
    error_domain: Quark,
    error_code: i32,
    dbus_error_name: &str,
) -> bool {
    let mut reg = REGISTRY.lock();
    // Look up by pair; verify the name matches (upstream does the same).
    let Some(re) = reg.by_pair.get(&(error_domain, error_code)) else {
        return false;
    };
    if re.dbus_error_name != dbus_error_name {
        return false;
    }
    reg.by_pair.remove(&(error_domain, error_code));
    reg.by_name.remove(dbus_error_name);
    true
}

/// Register a whole error domain from an entries table
/// (`g_dbus_error_register_error_domain`).
///
/// `quark_name` is the quark string for the domain; `quark` is the
/// pre-resolved quark value (upstream uses a `volatile gsize` +
/// `g_once_init_enter` pattern; we resolve the quark once at the call
/// site and pass it in). Every entry in `entries` is registered via
/// `dbus_error_register_error`.
pub fn dbus_error_register_error_domain(
    quark_name: &str,
    quark: Quark,
    entries: &[DBusErrorEntry],
) {
    // Resolve the quark from the name (idempotent — quark_from_string
    // returns the existing quark if already interned).
    let _ = quark_from_string(Some(quark_name));
    for entry in entries {
        // Upstream uses g_warn_if_fail on the register result; we
        // silently skip duplicates to keep the no_std API panic-free.
        let _ = dbus_error_register_error(quark, entry.error_code, entry.dbus_error_name);
    }
}

// ─────────────────────── remote error helpers ─────────────────────────────

/// Prefix that GIO prepends to remote D-Bus error messages.
const REMOTE_ERROR_PREFIX: &str = "GDBus.Error:";

/// Check if `error` represents a remote D-Bus error
/// (`g_dbus_error_is_remote_error`).
///
/// Returns `true` if the error message starts with `"GDBus.Error:"`.
pub fn dbus_error_is_remote_error(error: &Error) -> bool {
    str_has_prefix(error.message(), REMOTE_ERROR_PREFIX)
}

/// Extract the D-Bus error name from `error`
/// (`g_dbus_error_get_remote_error`).
///
/// If the error's `(domain, code)` is registered, returns the
/// registered name. Otherwise, if the message starts with
/// `"GDBus.Error:NAME: "`, returns `NAME`. Returns `None` if no
/// D-Bus error name can be recovered.
pub fn dbus_error_get_remote_error(error: &Error) -> Option<String> {
    // Ensure the well-known G_DBUS_ERROR entries are registered so
    // lookups by (domain, code) work for the standard domain.
    let _ = dbus_error_quark();

    let reg = REGISTRY.lock();
    if let Some(re) = reg.by_pair.get(&(error.domain(), error.code())) {
        return Some(re.dbus_error_name.clone());
    }
    drop(reg);

    // Fall back to parsing the "GDBus.Error:NAME: " prefix.
    parse_remote_prefix(error.message()).map(|(name, _)| name.to_owned())
}

/// Strip the `"GDBus.Error:NAME: "` prefix from `error`'s message
/// (`g_dbus_error_strip_remote_error`).
///
/// Returns `true` if the prefix was found and stripped, `false`
/// otherwise. Mutates `error` in place via `Error::set_message`.
pub fn dbus_error_strip_remote_error(error: &mut Error) -> bool {
    if let Some((_, rest)) = parse_remote_prefix(error.message()) {
        error.set_message(rest.to_owned());
        true
    } else {
        false
    }
}

/// Parse the `"GDBus.Error:NAME: REST"` prefix, returning
/// `(NAME, REST)` on success.
///
/// Public so the kernel smoke check can exercise it; not part of the
/// upstream public API (upstream inlines this into
/// `g_dbus_error_get_remote_error` / `_strip_remote_error`).
pub fn parse_remote_prefix(message: &str) -> Option<(&str, &str)> {
    if !str_has_prefix(message, REMOTE_ERROR_PREFIX) {
        return None;
    }
    let after_prefix = &message[REMOTE_ERROR_PREFIX.len()..];
    // Find the next ':' that's followed by ' '. Upstream uses strstr
    // for this; we scan bytes.
    let bytes = after_prefix.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            // SAFETY: we're splitting at valid UTF-8 boundaries
            // (ASCII ':' and ' '), so the slices are valid UTF-8.
            let name = &after_prefix[..i];
            let rest = &after_prefix[i + 2..];
            return Some((name, rest));
        }
        i += 1;
    }
    None
}

// ────────────────────── new_for_dbus_error ────────────────────────────────

/// Build a `glib_native::Error` from a D-Bus error name + message
/// (`g_dbus_error_new_for_dbus_error`).
///
/// If `dbus_error_name` is registered (via `dbus_error_register_error`
/// or `dbus_error_register_error_domain`), the returned `Error` uses
/// the registered `(domain, code)`. Otherwise, if the name is in the
/// `org.gtk.GDBus.UnmappedGError.Quark._*` form produced by
/// `dbus_error_encode_gerror`, the encoded `(domain, code)` is
/// decoded and used. Otherwise, the error is created with a
/// synthetic domain (`IO_ERROR_QUARK` placeholder) and code 0 —
/// matching the upstream fallback to `G_IO_ERROR_DBUS_ERROR`, which
/// we don't have access to here without porting GIOErrorEnum.
///
/// In all cases the message is prefixed with
/// `"GDBus.Error:NAME: "` so `dbus_error_get_remote_error` can
/// recover the name later.
pub fn dbus_error_new_for_dbus_error(dbus_error_name: &str, dbus_error_message: &str) -> Error {
    // Ensure well-known entries are registered.
    let _ = dbus_error_quark();

    let reg = REGISTRY.lock();
    if let Some(re) = reg.by_name.get(dbus_error_name) {
        let domain = re.error_domain;
        let code = re.error_code;
        drop(reg);
        return Error::new(
            domain,
            code,
            format!("GDBus.Error:{dbus_error_name}: {dbus_error_message}"),
        );
    }
    drop(reg);

    // Try to decode the org.gtk.GDBus.UnmappedGError.Quark._ form.
    if let Some((domain, code)) = decode_unmapped_gerror(dbus_error_name) {
        return Error::new(
            domain,
            code,
            format!("GDBus.Error:{dbus_error_name}: {dbus_error_message}"),
        );
    }

    // Fallback: use a synthetic domain. Upstream uses G_IO_ERROR /
    // G_IO_ERROR_DBUS_ERROR; we don't have GIOErrorEnum ported yet, so
    // register a dedicated quark for the fallback.
    let fallback_domain = quark_from_static_string(Some("g-dbus-error-fallback-quark"));
    Error::new(
        fallback_domain,
        0,
        format!("GDBus.Error:{dbus_error_name}: {dbus_error_message}"),
    )
}

// ─────────────────────── set_dbus_error ───────────────────────────────────

/// Format a printf-style `format` string substituting `%s` placeholders
/// with the provided string `args` (in order). `%%` is emitted literally.
///
/// This is a self-contained minimal formatter covering the substitution
/// patterns D-Bus error messages use (`%s` and `%%`). The crate's
/// `printf::printf_format` helper only accepts `&'static str` arguments
/// (via `PrintfArg::String`), which is unsuitable for runtime-built
/// messages, so a local helper is used instead. Other specifiers (`%d`,
/// `%u`, `%x`, `%f`, `%c`) are emitted verbatim (the `%` plus the
/// specifier character) rather than interpreted.
fn format_with_str_args(message_format: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut it = args.iter();
    let bytes = message_format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b's' => {
                    if let Some(arg) = it.next() {
                        out.push_str(arg);
                    }
                    i += 2;
                    continue;
                }
                b'%' => {
                    out.push('%');
                    i += 2;
                    continue;
                }
                _ => {
                    // Unknown specifier: emit the '%' verbatim and let the
                    // next iteration copy the specifier char.
                    out.push('%');
                    i += 1;
                    continue;
                }
            }
        }
        let ch_len = utf8_char_len_local(bytes[i]);
        let end = core::cmp::min(i + ch_len, bytes.len());
        if let Ok(s) = core::str::from_utf8(&bytes[i..end]) {
            out.push_str(s);
        }
        i = end;
    }
    out
}

/// Minimal UTF-8 leading-byte length helper (mirrors `markup::utf8_char_len`).
fn utf8_char_len_local(b: u8) -> usize {
    if b < 0xC0 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Set `error` to a D-Bus error (`g_dbus_error_set_dbus_error`).
///
/// Builds the message by printf-formatting `message_format` with `args`
/// (substituting each `%s` in turn), then composes
/// `"{dbus_error_message}: {formatted}"` and stores it via
/// [`dbus_error_new_for_dbus_error`].
///
/// # Deviation from upstream
///
/// Upstream takes a `GError **` and only assigns when `*error == NULL`
/// (emitting a "set over the top" warning otherwise). This binding takes
/// `&mut Error` (an already-initialized slot) and overwrites it
/// unconditionally, which is the natural Rust mapping for a non-`Option`
/// out-parameter. Callers wanting the "only if unset" guard should check
/// before calling.
///
/// `args` is a `&[&str]` slice rather than C varargs — only `%s`
/// substitution is supported (sufficient for D-Bus error messages). For
/// richer formatting use [`dbus_error_set_dbus_error_valist`] with a
/// `core::fmt::Arguments` value built via `format_args!`.
pub fn dbus_error_set_dbus_error(
    error: &mut Error,
    dbus_error_name: &str,
    dbus_error_message: &str,
    message_format: &str,
    args: &[&str],
) {
    let formatted = format_with_str_args(message_format, args);
    let combined = format!("{}: {}", dbus_error_message, formatted);
    *error = dbus_error_new_for_dbus_error(dbus_error_name, &combined);
}

/// Set `error` to a D-Bus error from a `va_list`
/// (`g_dbus_error_set_dbus_error_valist`).
///
/// # Deviation from upstream
///
/// Rust has no C varargs, so the valist is modelled as a
/// `core::fmt::Arguments` value. The formatted piece is obtained with
/// `format!("{}", args)`, i.e. the caller pre-assembles the formatted
/// string via `format_args!("...", ...)` exactly as `g_strdup_vprintf`
/// would have. `message_format` is accepted for API parity with the
/// upstream `_valist` signature but is not used (the `Arguments` value
/// already encodes it); it is intentionally consumed via `let _` to avoid
/// an unused-parameter warning.
///
/// Like [`dbus_error_set_dbus_error`], this overwrites `error`
/// unconditionally.
pub fn dbus_error_set_dbus_error_valist(
    error: &mut Error,
    dbus_error_name: &str,
    dbus_error_message: &str,
    message_format: &str,
    args: core::fmt::Arguments,
) {
    let _ = message_format;
    let formatted = format!("{}", args);
    let combined = format!("{}: {}", dbus_error_message, formatted);
    *error = dbus_error_new_for_dbus_error(dbus_error_name, &combined);
}

// ─────────────────────── encode_gerror ────────────────────────────────────

/// Prefix used by `dbus_error_encode_gerror` for errors whose domain
/// isn't registered with a D-Bus name. Matches upstream exactly so
/// interop with `dbus_error_new_for_dbus_error` (and upstream GIO)
/// works.
const UNMAPPED_PREFIX: &str = "org.gtk.GDBus.UnmappedGError.Quark._";

/// Encode an `Error` as a D-Bus error name string
/// (`g_dbus_error_encode_gerror`).
///
/// If the error's `(domain, code)` is registered, returns the
/// registered D-Bus name. Otherwise, encodes the domain quark name
/// and code into the
/// `org.gtk.GDBus.UnmappedGError.Quark._QUARKNAME.CodeCODE` form.
pub fn dbus_error_encode_gerror(error: &Error) -> String {
    // Ensure well-known entries are registered.
    let _ = dbus_error_quark();

    let reg = REGISTRY.lock();
    if let Some(re) = reg.by_pair.get(&(error.domain(), error.code())) {
        return re.dbus_error_name.clone();
    }
    drop(reg);

    // Encode as org.gtk.GDBus.UnmappedGError.Quark._<quark-name>.Code<code>
    // Upstream hex-encodes non-alphanumeric chars in the quark name
    // using _XX escapes. We do the same.
    let quark_name = crate::quark::quark_to_string(error.domain()).unwrap_or("");
    let mut encoded = String::from(UNMAPPED_PREFIX);
    for &b in quark_name.as_bytes() {
        if b.is_ascii_alphanumeric() {
            encoded.push(b as char);
        } else {
            encoded.push('_');
            encoded.push_str(&hex_digit((b >> 4) & 0xf));
            encoded.push_str(&hex_digit(b & 0xf));
        }
    }
    encoded.push_str(".Code");
    encoded.push_str(&error.code().to_string());
    encoded
}

fn hex_digit(n: u8) -> &'static str {
    match n {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        0xa => "a",
        0xb => "b",
        0xc => "c",
        0xd => "d",
        0xe => "e",
        _ => "f",
    }
}

/// Decode the `org.gtk.GDBus.UnmappedGError.Quark._*` form back into
/// `(domain, code)`. Returns `None` if `dbus_name` isn't in that form
/// or is malformed. Mirrors `_g_dbus_error_decode_gerror` upstream.
fn decode_unmapped_gerror(dbus_name: &str) -> Option<(Quark, i32)> {
    if !str_has_prefix(dbus_name, UNMAPPED_PREFIX) {
        return None;
    }
    let after = &dbus_name[UNMAPPED_PREFIX.len()..];
    let bytes = after.as_bytes();
    let mut quark_str = String::new();
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'.' {
        if bytes[i].is_ascii_alphanumeric() {
            quark_str.push(bytes[i] as char);
            i += 1;
        } else if bytes[i] == b'_' {
            // Hex escape: _XX
            i += 1;
            if i + 1 >= bytes.len() {
                return None;
            }
            let hi = hex_value(bytes[i])?;
            i += 1;
            let lo = hex_value(bytes[i])?;
            i += 1;
            quark_str.push((hi << 4 | lo) as char);
        } else {
            return None;
        }
    }
    // Expect ".Code" followed by the code number.
    let rest = &after[i..];
    if !str_has_prefix(rest, ".Code") {
        return None;
    }
    let code_str = &rest[".Code".len()..];
    let code: i32 = code_str.parse().ok()?;
    let domain = quark_from_string(Some(&quark_str));
    Some((domain, code))
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_enum_values_match_upstream() {
        assert_eq!(DBusError::Failed as i32, 0);
        assert_eq!(DBusError::NoMemory as i32, 1);
        assert_eq!(DBusError::PropertyReadOnly as i32, 44);
    }

    #[test]
    fn error_to_dbus_name() {
        assert_eq!(
            DBusError::Failed.to_dbus_name(),
            "org.freedesktop.DBus.Error.Failed"
        );
        assert_eq!(
            DBusError::PropertyReadOnly.to_dbus_name(),
            "org.freedesktop.DBus.Error.PropertyReadOnly"
        );
        assert_eq!(
            DBusError::SpawnExecFailed.to_dbus_name(),
            "org.freedesktop.DBus.Error.Spawn.ExecFailed"
        );
    }

    #[test]
    fn error_to_code() {
        assert_eq!(DBusError::Failed.to_code(), 0);
        assert_eq!(DBusError::Timeout.to_code(), 12);
        assert_eq!(DBusError::UnknownObject.to_code(), 41);
    }

    #[test]
    fn quark_is_nonzero_and_stable() {
        let q1 = dbus_error_quark();
        let q2 = dbus_error_quark();
        assert!(q1 > 0);
        assert_eq!(q1, q2);
    }

    #[test]
    fn well_known_entries_are_registered_after_quark_call() {
        let q = dbus_error_quark();
        // Failed (code 0) should be registered.
        let reg = REGISTRY.lock();
        let re = reg.by_pair.get(&(q, 0)).expect("Failed not registered");
        assert_eq!(re.dbus_error_name, "org.freedesktop.DBus.Error.Failed");
        // Reverse lookup.
        let re2 = reg
            .by_name
            .get("org.freedesktop.DBus.Error.PropertyReadOnly")
            .expect("PropertyReadOnly not registered");
        assert_eq!(re2.error_code, 44);
    }

    #[test]
    fn register_and_unregister_custom_error() {
        let test_domain = quark_from_static_string(Some("test-custom-domain"));
        let code = 42i32;
        let name = "org.test.CustomError";

        // Clean slate: ensure not already registered.
        let _ = dbus_error_unregister_error(test_domain, code, name);

        assert!(dbus_error_register_error(test_domain, code, name));
        // Re-registering the same pair or name should fail.
        assert!(!dbus_error_register_error(test_domain, code, name));
        assert!(!dbus_error_register_error(test_domain, code + 1, name));
        assert!(!dbus_error_register_error(
            test_domain,
            code,
            "org.test.DifferentName"
        ));

        // Unregister.
        assert!(dbus_error_unregister_error(test_domain, code, name));
        // Re-unregistering fails.
        assert!(!dbus_error_unregister_error(test_domain, code, name));

        // After unregister, we can re-register.
        assert!(dbus_error_register_error(test_domain, code, name));
        // Clean up.
        assert!(dbus_error_unregister_error(test_domain, code, name));
    }

    #[test]
    fn register_error_domain_registers_all_entries() {
        let domain = quark_from_static_string(Some("test-domain-entries"));
        let entries = [
            DBusErrorEntry {
                error_code: 1,
                dbus_error_name: "org.test.A",
            },
            DBusErrorEntry {
                error_code: 2,
                dbus_error_name: "org.test.B",
            },
            DBusErrorEntry {
                error_code: 3,
                dbus_error_name: "org.test.C",
            },
        ];
        dbus_error_register_error_domain("test-domain-entries", domain, &entries);
        let reg = REGISTRY.lock();
        assert!(reg.by_pair.get(&(domain, 1)).is_some());
        assert!(reg.by_pair.get(&(domain, 2)).is_some());
        assert!(reg.by_pair.get(&(domain, 3)).is_some());
        assert_eq!(reg.by_name.get("org.test.A").unwrap().error_code, 1);
        // Clean up.
        drop(reg);
        let _ = dbus_error_unregister_error(domain, 1, "org.test.A");
        let _ = dbus_error_unregister_error(domain, 2, "org.test.B");
        let _ = dbus_error_unregister_error(domain, 3, "org.test.C");
    }

    #[test]
    fn is_remote_error_detects_prefix() {
        let q = dbus_error_quark();
        let remote = Error::new(q, 0, "GDBus.Error:org.test.X: something failed");
        let local = Error::new(q, 0, "just a local error");
        assert!(dbus_error_is_remote_error(&remote));
        assert!(!dbus_error_is_remote_error(&local));
    }

    #[test]
    fn get_remote_error_returns_registered_name() {
        let q = dbus_error_quark();
        // Failed (code 0) is registered as org.freedesktop.DBus.Error.Failed.
        let err = Error::new(q, 0, "GDBus.Error:org.freedesktop.DBus.Error.Failed: boom");
        assert_eq!(
            dbus_error_get_remote_error(&err).as_deref(),
            Some("org.freedesktop.DBus.Error.Failed")
        );
    }

    #[test]
    fn get_remote_error_parses_prefix_for_unregistered() {
        // Use a domain/code that's not registered.
        let q = quark_from_static_string(Some("test-unregistered-domain"));
        let err = Error::new(q, 99, "GDBus.Error:org.test.NotRegistered: boom");
        assert_eq!(
            dbus_error_get_remote_error(&err).as_deref(),
            Some("org.test.NotRegistered")
        );
    }

    #[test]
    fn get_remote_error_returns_none_for_non_remote() {
        let q = quark_from_static_string(Some("test-non-remote"));
        let err = Error::new(q, 1, "just a local error");
        assert!(dbus_error_get_remote_error(&err).is_none());
    }

    #[test]
    fn strip_remote_error_removes_prefix() {
        let q = quark_from_static_string(Some("test-strip"));
        let mut err = Error::new(q, 1, "GDBus.Error:org.test.X: the real message");
        assert!(dbus_error_strip_remote_error(&mut err));
        assert_eq!(err.message(), "the real message");
    }

    #[test]
    fn strip_remote_error_returns_false_for_local() {
        let q = quark_from_static_string(Some("test-strip-noop"));
        let mut err = Error::new(q, 1, "local error");
        assert!(!dbus_error_strip_remote_error(&mut err));
        assert_eq!(err.message(), "local error");
    }

    #[test]
    fn new_for_dbus_error_uses_registered_domain() {
        let q = dbus_error_quark();
        let err = dbus_error_new_for_dbus_error("org.freedesktop.DBus.Error.Failed", "boom");
        assert_eq!(err.domain(), q);
        assert_eq!(err.code(), 0);
        assert!(err.message().contains("GDBus.Error:"));
        assert!(err.message().contains("org.freedesktop.DBus.Error.Failed"));
        assert!(err.message().ends_with("boom"));
    }

    #[test]
    fn new_for_dbus_error_falls_back_for_unregistered() {
        let err = dbus_error_new_for_dbus_error("org.test.NotInRegistry", "mystery");
        // Domain should be the fallback quark (non-zero).
        assert!(err.domain() != 0);
        assert!(err.message().contains("org.test.NotInRegistry"));
        assert!(err.message().contains("mystery"));
        // The error name should be recoverable.
        assert_eq!(
            dbus_error_get_remote_error(&err).as_deref(),
            Some("org.test.NotInRegistry")
        );
    }

    #[test]
    fn encode_gerror_returns_registered_name() {
        let q = dbus_error_quark();
        let err = Error::new(q, DBusError::Failed as i32, "boom");
        assert_eq!(
            dbus_error_encode_gerror(&err),
            "org.freedesktop.DBus.Error.Failed"
        );
    }

    #[test]
    fn encode_gerror_unmapped_form_roundtrips() {
        // Use a domain that's NOT registered with a D-Bus name.
        let domain = quark_from_string(Some("test-encode-roundtrip"));
        let code = 7i32;
        let err = Error::new(domain, code, "boom");
        let encoded = dbus_error_encode_gerror(&err);
        assert!(encoded.starts_with("org.gtk.GDBus.UnmappedGError.Quark._"));
        // Hyphens are hex-escaped as _2d, so the literal "test-encode-roundtrip"
        // appears as "test_2dencode_2droundtrip".
        assert!(encoded.contains("test_2dencode_2droundtrip"));
        assert!(encoded.ends_with(".Code7"));

        // Decode it back.
        let (decoded_domain, decoded_code) =
            decode_unmapped_gerror(&encoded).expect("round-trip decode should succeed");
        assert_eq!(decoded_domain, domain);
        assert_eq!(decoded_code, code);
    }

    #[test]
    fn encode_gerror_hex_escapes_non_alphanumeric() {
        // A quark name with a hyphen should be hex-escaped.
        let domain = quark_from_string(Some("test-hyphen-name"));
        let err = Error::new(domain, 3, "boom");
        let encoded = dbus_error_encode_gerror(&err);
        // '-' (0x2d) should appear as _2d.
        assert!(encoded.contains("_2d"));
        // And it should round-trip.
        let (d, c) = decode_unmapped_gerror(&encoded).unwrap();
        assert_eq!(d, domain);
        assert_eq!(c, 3);
    }

    #[test]
    fn new_for_dbus_error_decodes_unmapped_form() {
        // Use a domain name with no hyphens so the encoded form is
        // straightforward (the encoder hex-escapes non-alphanumeric
        // chars as _XX).
        let domain = quark_from_string(Some("testNewfromUnmapped"));
        let encoded = format!("org.gtk.GDBus.UnmappedGError.Quark._testNewfromUnmapped.Code5");
        let err = dbus_error_new_for_dbus_error(&encoded, "the message");
        assert_eq!(err.domain(), domain);
        assert_eq!(err.code(), 5);
        assert!(err.message().ends_with("the message"));
    }

    #[test]
    fn new_for_dbus_error_decodes_unmapped_form_with_escapes() {
        // Domain name with hyphens — the decoder must reverse the _2d
        // escapes back to '-'.
        let domain = quark_from_string(Some("test-hyphen-name"));
        let encoded = format!("org.gtk.GDBus.UnmappedGError.Quark._test_2dhyphen_2dname.Code5");
        let err = dbus_error_new_for_dbus_error(&encoded, "the message");
        assert_eq!(err.domain(), domain);
        assert_eq!(err.code(), 5);
    }

    #[test]
    fn parse_remote_prefix_handles_colons_in_name() {
        // The upstream parser finds the first ':' followed by ' '.
        // D-Bus error names can't contain ':', but the message can.
        let msg = "GDBus.Error:org.freedesktop.DBus.Error.Failed: error: with: colons";
        let (name, rest) = parse_remote_prefix(msg).expect("should parse");
        assert_eq!(name, "org.freedesktop.DBus.Error.Failed");
        assert_eq!(rest, "error: with: colons");
    }

    #[test]
    fn parse_remote_prefix_returns_none_for_no_prefix() {
        assert!(parse_remote_prefix("no prefix here").is_none());
    }

    #[test]
    fn parse_remote_prefix_returns_none_for_no_separator() {
        // Prefix present but no ": " separator after it.
        assert!(parse_remote_prefix("GDBus.Error:justname").is_none());
    }

    // ── set_dbus_error ──

    #[test]
    fn set_dbus_error_composes_message_with_percent_s() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 99, "placeholder");
        dbus_error_set_dbus_error(
            &mut err,
            "org.freedesktop.DBus.Error.Failed",
            "the message",
            "code %s",
            &["7"],
        );
        // Composed message: "GDBus.Error:NAME: dbus_error_message: formatted"
        assert_eq!(
            err.message(),
            "GDBus.Error:org.freedesktop.DBus.Error.Failed: the message: code 7"
        );
    }

    #[test]
    fn set_dbus_error_domain_code_match_new_for_dbus_error() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 99, "placeholder");
        dbus_error_set_dbus_error(
            &mut err,
            "org.freedesktop.DBus.Error.Failed",
            "the message",
            "code %s",
            &["7"],
        );
        // Should match dbus_error_new_for_dbus_error for the same name and
        // the same composed inner message.
        let reference = dbus_error_new_for_dbus_error(
            "org.freedesktop.DBus.Error.Failed",
            "the message: code 7",
        );
        assert_eq!(err.domain(), reference.domain());
        assert_eq!(err.code(), reference.code());
        // Well-known name resolves to the G_DBUS_ERROR quark + code 0.
        assert_eq!(err.domain(), q);
        assert_eq!(err.code(), 0);
    }

    #[test]
    fn set_dbus_error_multiple_string_args() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 0, "x");
        dbus_error_set_dbus_error(
            &mut err,
            "org.freedesktop.DBus.Error.InvalidArgs",
            "bad args",
            "%s and %s",
            &["foo", "bar"],
        );
        assert_eq!(
            err.message(),
            "GDBus.Error:org.freedesktop.DBus.Error.InvalidArgs: bad args: foo and bar"
        );
    }

    #[test]
    fn set_dbus_error_percent_percent_is_literal() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 0, "x");
        dbus_error_set_dbus_error(
            &mut err,
            "org.freedesktop.DBus.Error.Failed",
            "msg",
            "100%% done",
            &[],
        );
        assert_eq!(
            err.message(),
            "GDBus.Error:org.freedesktop.DBus.Error.Failed: msg: 100% done"
        );
    }

    #[test]
    fn set_dbus_error_valist_uses_arguments() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 0, "x");
        dbus_error_set_dbus_error_valist(
            &mut err,
            "org.freedesktop.DBus.Error.Failed",
            "msg",
            "val %d",
            format_args!("val {}", 5),
        );
        assert_eq!(
            err.message(),
            "GDBus.Error:org.freedesktop.DBus.Error.Failed: msg: val 5"
        );
        // Domain/code parity with new_for_dbus_error.
        let reference =
            dbus_error_new_for_dbus_error("org.freedesktop.DBus.Error.Failed", "msg: val 5");
        assert_eq!(err.domain(), reference.domain());
        assert_eq!(err.code(), reference.code());
    }

    #[test]
    fn set_dbus_error_overwrites_existing_error() {
        let q = dbus_error_quark();
        let mut err = Error::new(q, 0, "previous");
        dbus_error_set_dbus_error(
            &mut err,
            "org.freedesktop.DBus.Error.TimedOut",
            "timed out",
            "after %s ms",
            &["500"],
        );
        // The previous message is replaced (Rust &mut Error semantics).
        assert_eq!(
            err.message(),
            "GDBus.Error:org.freedesktop.DBus.Error.TimedOut: timed out: after 500 ms"
        );
        assert_eq!(err.code(), DBusError::TimedOut as i32);
    }
}
