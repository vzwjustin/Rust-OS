//! Native Rust reimplementation of GLib.
//!
//! See [`docs/rust-migration.md`](../../docs/rust-migration.md) for the phased
//! migration plan.

// Dual-mode: `no_std` for the kernel; full `std` under `cargo test` so the
// existing test suite (thread_local!, std prelude) keeps working.
#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

/// `eprintln!`-style diagnostic. Prints to stderr under `std`/tests; on bare
/// `no_std` (the kernel) it compiles to nothing.
// ponytail: no-op on no_std. Upgrade path: route to `crate::messages` or a
// RustOS log hook if these warnings need to surface in the kernel.
macro_rules! gwarn {
    ($($arg:tt)*) => {{
        #[cfg(test)]
        ::std::eprintln!($($arg)*);
    }};
}

/// Internal prelude: the `alloc` types and traits that live in `std`'s prelude
/// but not `core`'s. Glob-imported by every module so `no_std` code reads like
/// `std` code without per-item imports.
pub(crate) mod prelude {
    pub(crate) use alloc::borrow::ToOwned;
    pub(crate) use alloc::boxed::Box;
    pub(crate) use alloc::string::{String, ToString};
    pub(crate) use alloc::vec::Vec;
    pub(crate) use core::fmt::Write;
}

pub mod array;
pub mod asyncqueue;
pub mod atomic;
pub mod base64;
pub mod bitlock;
pub mod bytes;
pub mod cache;
pub mod charset;
pub mod checked;
pub mod completion;
pub mod checksum;
pub mod convert;
pub mod dataset;
pub mod date;
pub mod datetime;
pub mod dir;
pub mod endian;
pub mod environ;
pub mod error;
pub mod fileutils;
pub mod gobject;
pub mod gmodule;
pub mod gparamspec;
pub mod gsignal;
pub mod gstring;
pub mod gtype;
pub mod gvalue;
pub mod gfileattribute;
pub mod gdbusintrospection;
pub mod gdbuserror;
pub mod gioerror;
pub mod gnotification;
pub mod gsrvtarget;
pub mod hash;
pub mod hook;
pub mod hmac;
pub mod hostutils;
pub mod iochannel;
pub mod keyfile;
pub mod list;
pub mod mainloop;
pub mod mappedfile;
pub mod mem;
pub mod messages;
pub mod markup;
pub mod node;
pub mod option;
pub mod pathbuf;
pub mod pattern;
pub mod poll;
pub mod printf;
pub mod primes;
pub mod ptr_array;
pub mod quark;
pub mod queue;
pub mod qsort;
pub mod rand;
pub mod rcbox;
pub mod refcount;
pub mod regex;
pub mod relation;
pub mod refstring;
pub mod scanner;
pub mod sequence;
pub mod shell;
pub mod slice;
pub mod spawn;
pub mod stdio;
pub mod strfuncs;
pub mod stringchunk;
pub mod strvbuilder;
pub mod timer;
pub mod testutils;
pub mod thread;
pub mod threadpool;
pub mod tree;
pub mod timezone;
pub mod trashstack;
pub mod unicode;
pub mod uri;
pub mod utf8;
pub mod utils;
pub mod uuid;
pub mod variant;
pub mod varianttype;
pub mod version;

#[cfg(test)]
extern crate std;

pub use array::{ByteArray, GArray};
pub use asyncqueue::{AsyncQueue, async_queue_new};
pub use atomic::{AtomicInt, AtomicPointer, AtomicUInt};
pub use bytes::Bytes;
pub use cache::{Cache, CacheNewFunc, CacheDupFunc, CacheDestroyFunc};
pub use checked::{checked_add_size, checked_add_u32, checked_mul_size, checked_mul_u32};
pub use completion::{Completion, CompletionFunc, CompletionStrncmpFunc};
pub use dir::{Dir, DirError, DirPlatform, NoDirPlatform};
pub use dataset::{
    datalist_clear, datalist_foreach, datalist_id_get_data, datalist_id_remove_no_notify,
    datalist_id_set_data, datalist_id_set_data_full, datalist_init, DataList,
};
pub use endian::{
    g_htonl, g_htons, g_ntohl, g_ntohs, swap_u16_le_be, swap_u32_le_be, swap_u64_le_be,
};
pub use error::{
    clear_error, error_copy, error_free, error_matches, error_new, error_new_literal, prefix_error,
    prefix_error_literal, propagate_error, propagate_prefixed_error, set_error, set_error_literal,
    steal_error, Error,
};
pub use gstring::GString;
pub use iochannel::{
    IOError, IOChannelError, IOStatus, SeekType, IOFlags,
    io_channel_error_quark,
};
pub use hash::{
    direct_equal, direct_hash, double_equal, double_hash, int64_equal, int64_hash, int_equal,
    int_hash, str_equal, str_hash, HashTable, HashTableIter,
};
pub use list::{CompareFn, GList, GSList, List, SList};
pub use mainloop::{
    MainContext, MainContextFlags, MainLoop, Source, SourceFuncs, SourceFlags,
    SourceFunc, SourcePrepareFunc, SourceCheckFunc, SourceDispatchFunc,
    SourceFinalizeFunc, SourceCallbackFuncs,
    SOURCE_CONTINUE, SOURCE_REMOVE,
    default_context, timeout_add, idle_add, source_remove,
};
pub use mappedfile::{MappedFile, MappedFileError, MappedFilePlatform, NoMappedFilePlatform};
pub use mem::{
    aligned_alloc, aligned_alloc0, clear, clear_with, free, malloc, malloc0, malloc0_n, malloc_n,
    memdup, memdup2, realloc, realloc_n, steal, try_aligned_alloc, try_malloc, try_malloc0,
    try_malloc0_n, try_malloc_n, try_realloc, try_realloc_n, AlignedBuffer, MEM_ALIGN,
};
pub use messages::{
    critical, debug, info, log, log_default_handler, log_fmt, log_remove_handler,
    log_set_default_handler, log_set_handler, message, print, printerr, set_print_handler,
    set_printerr_handler, warning, LogFunc, LogLevelFlags, PrintFunc,
};
pub use option::{
    option_context_new, option_error_quark, option_group_new, OptionArg, OptionContext,
    OptionEntry, OptionError, OptionFlags, OptionGroup, OPTION_REMAINING,
};
pub use ptr_array::{GPointer, PtrArray, PtrCompareFunc};
pub use quark::{
    intern_static_string, intern_string, quark_from_static_string, quark_from_string,
    quark_to_string, quark_try_string, Quark,
};
pub use queue::GQueue;
pub use qsort::{sort_array, sort_array_unstable};
pub use rand::{
    Rand, random_int, random_int_range, random_double, random_double_range,
    random_boolean, random_set_seed,
};
pub use rcbox::{RcBox, AtomicRcBox, rc_box_alloc0, atomic_rc_box_alloc, atomic_rc_box_alloc0};
pub use refcount::{AtomicRefCount, RefCount};
pub use relation::{Relation, Tuples, Tuple};
pub use refstring::RefString;
pub use scanner::{Scanner, ScannerConfig, TokenType, TokenValue, ErrorType,
    CSET_A_2_Z, CSET_a_2_z, CSET_DIGITS};
pub use sequence::{Sequence, SequenceIter};
pub use strfuncs::{
    ascii_isalnum, ascii_isalpha, ascii_iscntrl, ascii_isdigit, ascii_isgraph,
    ascii_islower, ascii_isprint, ascii_ispunct, ascii_isspace, ascii_isupper,
    ascii_isxdigit, ascii_digit_value, ascii_strdown, ascii_strncasecmp, ascii_strtoll,
    ascii_strtoull, ascii_strup, ascii_tolower, ascii_toupper, ascii_xdigit_value,
    ascii_strcasecmp, str_is_ascii, str_has_prefix, str_has_suffix, strcasecmp, strchomp,
    strchug, strcmp, strcanon, strcompress, strconcat, strdelimit, strdup, strdupv,
    strescape, strjoin, strjoinv, strlen, strndup, strndup_str, strnfill, strreverse,
    strrstr, strstr_len, strsplit, strsplit_set, strstrip, strv_contains, strv_equal,
    strv_length,
};
pub use bitlock::{
    bit_lock, bit_trylock, bit_unlock, pointer_bit_lock, pointer_bit_trylock,
    pointer_bit_unlock,
};
pub use tree::{CompareDataFn, GTreeNode, TraverseFn, TraverseNodeFn, Tree};
pub use base64::{base64_decode, base64_decode_inplace, base64_encode, Base64Decoder, Base64Encoder};
pub use checksum::{
    compute_checksum_for_bytes, compute_checksum_for_data, compute_checksum_for_string,
    checksum_type_get_length, Checksum, ChecksumType,
};
pub use fileutils::{
    build_filename, build_pathv, canonicalize_filename, file_error_quark,
    is_dir_separator, path_get_basename, path_get_dirname, path_is_absolute,
    path_skip_root, FileError, FileTest,
};
pub use convert::{
    convert_error_quark, filename_display_basename, filename_display_name,
    filename_from_uri, filename_to_uri, uri_list_extract_uris, ConvertError,
};
pub use charset::{
    get_charset, get_codeset, get_console_charset, get_language_names,
    get_locale_variants,
};
pub use hmac::{
    compute_hmac_for_bytes, compute_hmac_for_data, compute_hmac_for_string, Hmac,
};
pub use hostutils::{
    hostname_is_ascii_encoded, hostname_is_ip_address, hostname_is_non_ascii,
    hostname_to_ascii, hostname_to_unicode,
};
pub use environ::{
    environ_getenv, environ_setenv, environ_unsetenv, get_environ, getenv,
    listenv, setenv, unsetenv,
};
pub use keyfile::{
    key_file_error_quark, KeyFile, KeyFileError, KeyFileFlags,
};
pub use date::{
    date_parse, get_days_in_month, is_leap_year, monday_weeks_in_year,
    sunday_weeks_in_year, valid_day, valid_dmy, valid_julian, valid_month,
    valid_weekday, valid_year, Date, DateDay, DateMonth, DateWeekday, DateYear,
    DATE_BAD_JULIAN,
};
pub use datetime::{
    DateTime, TimeSpan,
    TIME_SPAN_DAY, TIME_SPAN_HOUR, TIME_SPAN_MINUTE,
    TIME_SPAN_SECOND, TIME_SPAN_MILLISECOND,
};
pub use timezone::{TimeZone, TimeType};
pub use hook::{
    hook_compare_ids, Hook, HookCallback, HookCheckFunc, HookCompareFunc, HookFindFunc,
    HookFunc, HookList, DestroyNotify,
    HOOK_FLAG_ACTIVE, HOOK_FLAG_IN_CALL, HOOK_FLAG_MASK,
};
pub use utf8::{
    unichar_validate, unichar_to_utf8, unichar_to_utf8_len, unichar_to_utf8_string,
    utf8_get_char, utf8_len, utf8_strlen, utf8_validate, utf8_offset_to_pointer,
    utf8_pointer_to_offset, utf8_prev_char, utf8_next_char,
    unichar_isalnum, unichar_isalpha, unichar_iscntrl, unichar_isdigit,
    unichar_islower, unichar_isprint, unichar_ispunct,
    unichar_isspace, unichar_isupper, unichar_isxdigit,
    unichar_toupper, unichar_tolower, unichar_digit_value, unichar_xdigit_value,
    Unichar, Unichar2,
};
pub use unicode::{
    UnicodeType, UnicodeBreakType, NormalizeMode, UnicodeScript,
    combining_class as unicode_combining_class,
};
pub use varianttype::{
    type_string_is_valid, type_equal, type_hash, scan_type_string,
    VariantType, VariantClass,
    VARIANT_TYPE_BOOLEAN, VARIANT_TYPE_BYTE, VARIANT_TYPE_INT16,
    VARIANT_TYPE_UINT16, VARIANT_TYPE_INT32, VARIANT_TYPE_UINT32,
    VARIANT_TYPE_INT64, VARIANT_TYPE_UINT64, VARIANT_TYPE_DOUBLE,
    VARIANT_TYPE_STRING, VARIANT_TYPE_OBJECT_PATH, VARIANT_TYPE_SIGNATURE,
    VARIANT_TYPE_VARIANT, VARIANT_TYPE_HANDLE, VARIANT_TYPE_UNIT,
    VARIANT_TYPE_ANY, VARIANT_TYPE_BASIC, VARIANT_TYPE_MAYBE,
    VARIANT_TYPE_ARRAY, VARIANT_TYPE_TUPLE, VARIANT_TYPE_DICT_ENTRY,
    VARIANT_TYPE_DICTIONARY, VARIANT_TYPE_STRING_ARRAY,
    VARIANT_TYPE_BYTESTRING, VARIANT_TYPE_BYTESTRING_ARRAY,
    VARIANT_TYPE_VARDICT,
};
pub use variant::{
    Variant, VariantBuilder, VariantParseError,
    parse as variant_parse, variant_parse_error_quark,
};
pub use pattern::{PatternSpec, pattern_match_simple};
pub use pathbuf::PathBuf;
pub use poll::{PollFD, IOCondition, PollFunc};
pub use printf::{sprintf, vsprintf, printf_format, PrintfArg};
pub use primes::spaced_primes_closest;
pub use regex::{
    Regex, RegexError, RegexCompileFlags, RegexMatchFlags, MatchInfo,
    regex_error_quark,
};
pub use shell::{shell_quote, shell_unquote, shell_parse_argv, shell_error_quark, ShellError};
pub use slice::{
    SliceConfig, slice_alloc, slice_alloc0, slice_copy, slice_free1,
    slice_set_config, slice_get_config,
};
pub use spawn::{
    SpawnError, SpawnFlags, SpawnResult, SpawnChildSetupFunc, Pid,
    SpawnPlatform, NoSpawnPlatform,
    spawn_error_quark, spawn_exit_error_quark,
};
pub use stdio::{
    StatBuf, OpenFlags, StdioPlatform, NoStdioPlatform,
    F_OK, R_OK, W_OK, X_OK,
    S_IRWXU, S_IRUSR, S_IWUSR, S_IXUSR,
    S_IRWXG, S_IRGRP, S_IWGRP, S_IXGRP,
    S_IRWXO, S_IROTH, S_IWOTH, S_IXOTH,
};
pub use node::{Node, NTree, TraverseFlags, TraverseType};
pub use uri::{
    Uri, UriFlags, UriHideFlags, UriError, peek_scheme, join, is_valid,
    escape_string, unescape_string,
};
pub use stringchunk::StringChunk;
pub use strvbuilder::StrvBuilder;
pub use trashstack::TrashStack;
pub use markup::{
    MarkupParser, MarkupError, MarkupParseFlags, Attribute, Element, MarkupNode,
    escape_text, markup_error_quark,
};
pub use version::{
    GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION, GLIB_MICRO_VERSION,
    GLIB_INTERFACE_AGE, GLIB_BINARY_AGE,
    check_version, check_version_bool,
};
pub use timer::{Timer, set_clock as timer_set_clock, ClockFn};
pub use testutils::{
    TestCase, TestSuite, TestTrapFlags, TestTrapStatus, TestSubprocessFlags,
    test_init, test_run, test_add_func, test_add_data_func,
    test_create_suite, test_get_root,
    assert_true, assert_false, assert_cmpint, assert_cmpstr,
    assert_null, assert_nonnull,
    test_expect_message, test_assert_expected_messages,
    test_trap_subprocess,
};
pub use gtype::{
    GType, GTypeFlags, GTypeFundamentalFlags, GTypeInfo, GTypeValueTable,
    GValueData, TypeClassData, TypeInstanceData, TypeQuery,
    SignalDef, ParamSpec as GTypeParamSpec, ParamFlags as GTypeParamFlags,
    G_TYPE_INVALID, G_TYPE_NONE, G_TYPE_INTERFACE, G_TYPE_CHAR, G_TYPE_UCHAR,
    G_TYPE_BOOLEAN, G_TYPE_INT, G_TYPE_UINT, G_TYPE_LONG, G_TYPE_ULONG,
    G_TYPE_INT64, G_TYPE_UINT64, G_TYPE_ENUM, G_TYPE_FLAGS,
    G_TYPE_FLOAT, G_TYPE_DOUBLE, G_TYPE_STRING, G_TYPE_POINTER,
    G_TYPE_BOXED, G_TYPE_PARAM, G_TYPE_OBJECT, G_TYPE_VARIANT,
    G_TYPE_FUNDAMENTAL_MAX, G_TYPE_FUNDAMENTAL_SHIFT,
    g_type_make_fundamental,
    type_init, type_get_type_registration_serial,
    type_from_name, type_name, type_parent, type_fundamental,
    type_fundamental_next, type_is_a, type_depth,
    type_children, type_interfaces,
    type_is_classed, type_is_instantiatable, type_is_abstract, type_is_final,
    type_register_fundamental, type_register_static,
    type_register_static_simple,
    type_instance_size, type_class_size, type_value_table,
    type_add_interface, type_query, type_get_all,
};
pub use gvalue::{
    GValue, TransformFunc, default_value_table_for,
    value_new_boolean, value_new_int, value_new_uint, value_new_int64,
    value_new_uint64, value_new_float, value_new_double, value_new_string,
    value_new_char, value_new_enum, value_new_flags,
    value_new_pointer, value_new_object, value_new_boxed,
};
pub use gparamspec::{
    ParamSpec, ParamID, ParamFlags,
    install_properties, find_property, find_property_by_id, property_names,
};
pub use gsignal::{
    SignalID, HandlerID, SignalFlags, ConnectFlags, SignalCallback,
    SignalQuery,
    signal_new, signal_lookup, signal_query, signal_name,
    signal_connect, signal_connect_by_name,
    signal_handler_disconnect, signal_handler_is_connected,
    signal_handler_block, signal_handler_unblock,
    signal_emit, signal_emit_by_name,
    signal_list_ids, signal_n_handlers, signal_handlers_disconnect_all,
};
pub use gobject::{
    GObject, ObjectFlags, WeakRefCallback,
    object_new, object_new_with_params, PropertyBinding,
};
pub use gmodule::{
    GModule, GModuleFlags, GModuleError, GModuleCheckInit, GModuleUnload,
    ModuleHandle, ModulePlatform, NoModulePlatform,
    module_supported, module_open, module_open_full, module_close,
    module_make_resident, module_error, module_symbol, module_name,
    module_build_path, module_error_quark,
};
pub use gfileattribute::{
    FileAttributeType, FileAttributeInfoFlags, FileAttributeInfo,
    FileAttributeInfoList,
};
pub use gdbusintrospection::{
    DBusAnnotationInfo, DBusArgInfo, DBusMethodInfo, DBusSignalInfo,
    DBusPropertyInfo, DBusPropertyInfoFlags, DBusInterfaceInfo, DBusNodeInfo,
    dbus_annotation_info_lookup, dbus_interface_info_lookup_method,
    dbus_interface_info_lookup_signal, dbus_interface_info_lookup_property,
    dbus_node_info_lookup_interface,
};
pub use gdbuserror::{
    DBusError, DBusErrorEntry,
    dbus_error_quark, dbus_error_register_error, dbus_error_unregister_error,
    dbus_error_register_error_domain, dbus_error_is_remote_error,
    dbus_error_get_remote_error, dbus_error_strip_remote_error,
    dbus_error_new_for_dbus_error, dbus_error_encode_gerror,
};
pub use gioerror::{
    IOErrorEnum, io_error_quark, io_error_from_errno, io_error_from_file_error,
};
pub use fileutils::file_error_from_errno;
pub use gnotification::{
    Notification, NotificationPriority, NotificationButton, NotificationIcon,
};
pub use gsrvtarget::{SrvTarget, srv_target_list_sort};
pub use thread::{
    GMutex, GRecMutex, GRWLock, GCond, Once, OnceStatus, ThreadError,
    thread_error_quark,
};
pub use threadpool::{
    ThreadPool, ThreadPoolError,
    set_max_unused_threads, get_max_unused_threads,
    get_num_unused_threads, stop_unused_threads,
    set_max_idle_time, get_max_idle_time,
};
pub use utils::{
    get_prgname, set_prgname, get_application_name, set_application_name,
    OS_INFO_KEY_NAME, OS_INFO_KEY_PRETTY_NAME, OS_INFO_KEY_VERSION,
    OS_INFO_KEY_VERSION_CODENAME, OS_INFO_KEY_VERSION_ID, OS_INFO_KEY_ID,
    OS_INFO_KEY_HOME_URL, OS_INFO_KEY_DOCUMENTATION_URL, OS_INFO_KEY_SUPPORT_URL,
    OS_INFO_KEY_BUG_REPORT_URL, OS_INFO_KEY_PRIVACY_POLICY_URL,
    USEC_PER_SEC, NSEC_PER_SEC,
};
pub use uuid::{uuid_string_is_valid, uuid_string_random};

/// Alias matching GLib's `gboolean`: `true` or `false`.
pub type Bool = bool;

/// Alias matching GLib's `gsize`.
pub type Size = usize;

/// Alias matching GLib's `guint`.
pub type UInt = u32;

/// Mathematical constants from `gtypes.h`.
pub mod constants {
    /// Euler's number.
    pub const E: f64 = core::f64::consts::E;
    /// Natural log of 2.
    pub const LN_2: f64 = core::f64::consts::LN_2;
    /// Natural log of 10.
    pub const LN_10: f64 = core::f64::consts::LN_10;
    /// Pi.
    pub const PI: f64 = core::f64::consts::PI;
    /// Pi / 2.
    pub const PI_2: f64 = core::f64::consts::FRAC_PI_2;
    /// Pi / 4.
    pub const PI_4: f64 = core::f64::consts::FRAC_PI_4;
    /// Square root of 2.
    pub const SQRT_2: f64 = core::f64::consts::SQRT_2;
}
