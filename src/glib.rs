//! GLib compatibility layer for RustOS.
//!
//! Re-exports `glib_native` types and provides thin wrappers that adapt
//! GLib's no_std API to the kernel environment (e.g. routing log output
//! to the serial console instead of stdout/stderr).

#![allow(unused_imports)]

use alloc::borrow::ToOwned;
use alloc::string::ToString;
use alloc::vec;

pub use glib_native::*;

pub use glib_native::{
    // Memory
    aligned_alloc,
    aligned_alloc0,
    // Async queue
    async_queue_new,
    ascii_digit_value,
    // ASCII char classification
    ascii_isalnum,
    ascii_isalpha,
    ascii_iscntrl,
    ascii_isdigit,
    ascii_isgraph,
    ascii_islower,
    ascii_isprint,
    ascii_ispunct,
    ascii_isspace,
    ascii_isupper,
    ascii_isxdigit,
    // String functions
    ascii_strcasecmp,
    ascii_strdown,
    ascii_strncasecmp,
    ascii_strtoll,
    ascii_strtoull,
    ascii_strup,
    ascii_tolower,
    ascii_toupper,
    ascii_xdigit_value,
    // Base64
    base64_decode,
    base64_decode_inplace,
    base64_encode,
    // Bit locks
    bit_lock,
    bit_trylock,
    bit_unlock,
    build_filename,
    build_pathv,
    canonicalize_filename,
    // Checked arithmetic
    checked_add_size,
    checked_add_u32,
    checked_mul_size,
    checked_mul_u32,
    checksum_type_get_length,
    // Version
    check_version,
    check_version_bool,
    clear,
    clear_error,
    clear_with,
    compute_checksum_for_bytes,
    compute_checksum_for_data,
    compute_checksum_for_string,
    compute_hmac_for_bytes,
    compute_hmac_for_data,
    compute_hmac_for_string,
    convert_error_quark,
    critical,
    datalist_clear,
    datalist_foreach,
    datalist_id_get_data,
    datalist_id_remove_no_notify,
    datalist_id_set_data,
    datalist_id_set_data_full,
    datalist_init,
    date_parse,
    debug,
    // Hash functions
    direct_equal,
    direct_hash,
    double_equal,
    double_hash,
    // Environment
    environ_getenv,
    environ_setenv,
    environ_unsetenv,
    error_copy,
    error_free,
    error_matches,
    error_new,
    error_new_literal,
    // Markup
    escape_text,
    file_error_quark,
    filename_display_basename,
    filename_display_name,
    filename_from_uri,
    filename_to_uri,
    free,
    // Endian
    g_htonl,
    g_htons,
    g_ntohl,
    g_ntohs,
    // Charset
    get_charset,
    get_codeset,
    get_console_charset,
    get_days_in_month,
    get_environ,
    get_language_names,
    get_locale_variants,
    getenv,
    hook_compare_ids,
    // Hostname utilities
    hostname_is_ascii_encoded,
    hostname_is_ip_address,
    hostname_is_non_ascii,
    hostname_to_ascii,
    hostname_to_unicode,
    info,
    int64_equal,
    int64_hash,
    int_equal,
    int_hash,
    intern_static_string,
    intern_string,
    is_dir_separator,
    is_leap_year,
    key_file_error_quark,
    listenv,
    log,
    log_default_handler,
    log_fmt,
    log_remove_handler,
    log_set_default_handler,
    log_set_handler,
    malloc,
    // Markup
    markup_error_quark,
    malloc0,
    malloc0_n,
    malloc_n,
    memdup,
    memdup2,
    message,
    monday_weeks_in_year,
    option_context_new,
    option_error_quark,
    option_group_new,
    path_get_basename,
    path_get_dirname,
    path_is_absolute,
    path_skip_root,
    pattern_match_simple,
    pointer_bit_lock,
    pointer_bit_trylock,
    pointer_bit_unlock,
    prefix_error,
    prefix_error_literal,
    // Variant
    variant_parse,
    variant_parse_error_quark,
    // Utils
    get_application_name,
    get_prgname,
    set_application_name,
    set_prgname,
    timer_set_clock,
    // Thread
    thread_error_quark,
    // I/O channel
    io_channel_error_quark,
    print,
    printerr,
    propagate_error,
    propagate_prefixed_error,
    quark_from_static_string,
    quark_from_string,
    quark_to_string,
    quark_try_string,
    realloc,
    realloc_n,
    scan_type_string,
    set_error,
    set_error_literal,
    set_print_handler,
    set_printerr_handler,
    setenv,
    // Primes
    spaced_primes_closest,
    // Random
    random_int,
    random_int_range,
    random_double,
    random_double_range,
    random_boolean,
    random_set_seed,
    // Slice (deprecated)
    slice_alloc,
    slice_alloc0,
    slice_copy,
    slice_free1,
    slice_set_config,
    slice_get_config,
    // Sorting
    sort_array,
    sort_array_unstable,
    // Printf
    sprintf,
    vsprintf,
    printf_format,
    // UUID
    uuid_string_is_valid,
    uuid_string_random,
    // Main loop
    default_context,
    timeout_add,
    idle_add,
    source_remove,
    // Regex
    regex_error_quark,
    // Spawn
    spawn_error_quark,
    spawn_exit_error_quark,
    // Test utils
    test_init,
    test_run,
    test_add_func,
    test_add_data_func,
    test_create_suite,
    test_get_root,
    assert_true,
    assert_false,
    assert_cmpint,
    assert_cmpstr,
    assert_null,
    assert_nonnull,
    test_expect_message,
    test_assert_expected_messages,
    test_trap_subprocess,
    // Thread pool
    set_max_unused_threads,
    get_max_unused_threads,
    get_num_unused_threads,
    stop_unused_threads,
    set_max_idle_time,
    get_max_idle_time,
    // GType system
    g_type_make_fundamental,
    type_init,
    type_get_type_registration_serial,
    type_from_name,
    type_name,
    type_parent,
    type_fundamental,
    type_fundamental_next,
    type_is_a,
    type_depth,
    type_children,
    type_interfaces,
    type_is_classed,
    type_is_instantiatable,
    type_is_abstract,
    type_is_final,
    type_register_fundamental,
    type_register_static,
    type_register_static_simple,
    type_instance_size,
    type_class_size,
    type_value_table,
    type_add_interface,
    type_query,
    type_get_all,
    // GValue
    default_value_table_for,
    value_new_boolean,
    value_new_int,
    value_new_uint,
    value_new_int64,
    value_new_uint64,
    value_new_float,
    value_new_double,
    value_new_string,
    value_new_char,
    value_new_enum,
    value_new_flags,
    value_new_pointer,
    value_new_object,
    value_new_boxed,
    // GParamSpec
    install_properties,
    find_property,
    find_property_by_id,
    property_names,
    // GSignal
    signal_new,
    signal_lookup,
    signal_query,
    signal_name,
    signal_connect,
    signal_connect_by_name,
    signal_handler_disconnect,
    signal_handler_is_connected,
    signal_handler_block,
    signal_handler_unblock,
    signal_emit,
    signal_emit_by_name,
    signal_list_ids,
    signal_n_handlers,
    signal_handlers_disconnect_all,
    // GObject
    object_new,
    object_new_with_params,
    // GModule
    GModule,
    GModuleFlags,
    GModuleError,
    GModuleCheckInit,
    GModuleUnload,
    ModuleHandle,
    ModulePlatform,
    NoModulePlatform,
    module_supported,
    module_open,
    module_open_full,
    module_close,
    module_make_resident,
    module_error,
    module_symbol,
    module_name,
    module_build_path,
    module_error_quark,
    // URI functions
    escape_string,
    is_valid,
    join,
    peek_scheme,
    unescape_string,
    shell_error_quark,
    shell_parse_argv,
    shell_quote,
    shell_unquote,
    steal,
    steal_error,
    str_equal,
    str_has_prefix,
    str_has_suffix,
    str_hash,
    str_is_ascii,
    strcanon,
    strcasecmp,
    strchomp,
    strchug,
    strcmp,
    strcompress,
    strconcat,
    strdelimit,
    strdup,
    strdupv,
    strescape,
    strjoin,
    strjoinv,
    strlen,
    strndup,
    strndup_str,
    strnfill,
    strreverse,
    strrstr,
    strsplit,
    strsplit_set,
    strstr_len,
    strstrip,
    strv_contains,
    strv_equal,
    strv_length,
    sunday_weeks_in_year,
    swap_u16_le_be,
    swap_u32_le_be,
    swap_u64_le_be,
    try_aligned_alloc,
    try_malloc,
    try_malloc0,
    try_malloc0_n,
    try_malloc_n,
    try_realloc,
    try_realloc_n,
    type_equal,
    type_hash,
    type_string_is_valid,
    unichar_digit_value,
    unichar_isalnum,
    unichar_isalpha,
    unichar_iscntrl,
    unichar_isdigit,
    unichar_islower,
    unichar_isprint,
    unichar_ispunct,
    unichar_isspace,
    unichar_isupper,
    unichar_isxdigit,
    unichar_to_utf8,
    unichar_to_utf8_len,
    unichar_to_utf8_string,
    unichar_tolower,
    unichar_toupper,
    unichar_validate,
    unichar_xdigit_value,
    unsetenv,
    uri_list_extract_uris,
    utf8_get_char,
    utf8_len,
    utf8_next_char,
    utf8_offset_to_pointer,
    utf8_pointer_to_offset,
    utf8_prev_char,
    utf8_strlen,
    utf8_validate,
    valid_day,
    valid_dmy,
    valid_julian,
    valid_month,
    valid_weekday,
    valid_year,
    warning,
    AlignedBuffer,
    // Atomic types
    AtomicInt,
    AtomicPointer,
    // Refcount
    AtomicRefCount,
    AtomicUInt,
    // Async queue
    AsyncQueue,
    Base64Decoder,
    Base64Encoder,
    // Core types
    Bool,
    ByteArray,
    // Completion
    Completion,
    // Cache (deprecated)
    Cache,
    // Bytes
    Bytes,
    // Checksum
    Checksum,
    ChecksumType,
    // Tree types
    CompareDataFn,
    CompareFn,
    // Convert / URI helpers
    ConvertError,
    // Dataset
    DataList,
    // Date
    Date,
    DateDay,
    DateMonth,
    DateWeekday,
    DateYear,
    // Date-time
    DateTime,
    // Directory
    Dir,
    DirError,
    DirPlatform,
    NoDirPlatform,
    // Error
    Error,
    // File utilities
    FileError,
    FileTest,
    // Arrays
    GArray,
    // Lists
    GList,
    // Pointer arrays
    GPointer,
    // Queue
    GQueue,
    GSList,
    // GString
    GString,
    GTreeNode,
    // Hash
    HashTable,
    HashTableIter,
    // HMAC
    Hmac,
    // Hook list
    Hook,
    HookCallback,
    HookCheckFunc,
    HookCompareFunc,
    HookFindFunc,
    HookFunc,
    HookList,
    DestroyNotify,
    // I/O channel
    IOChannelError,
    IOCondition,
    IOError,
    IOFlags,
    IOStatus,
    // Key file (INI parser)
    KeyFile,
    KeyFileError,
    KeyFileFlags,
    List,
    // Logging
    LogFunc,
    LogLevelFlags,
    // Main loop
    MainContext,
    MainContextFlags,
    MainLoop,
    // Mapped file
    MappedFile,
    MappedFileError,
    MappedFilePlatform,
    NoMappedFilePlatform,
    // Markup (XML parser)
    MarkupError,
    MarkupNode,
    MarkupParseFlags,
    MarkupParser,
    // N-ary tree
    NTree,
    Node,
    // Option parsing
    OptionArg,
    OptionContext,
    OptionEntry,
    OptionError,
    OptionFlags,
    OptionGroup,
    // Pattern matching
    PatternSpec,
    // Path buffer
    PathBuf,
    // Poll
    PollFD,
    PrintFunc,
    PtrArray,
    PtrCompareFunc,
    // Printf
    PrintfArg,
    // Quarks
    Quark,
    // Random
    Rand,
    // RcBox
    RcBox,
    AtomicRcBox,
    // Regex
    Regex,
    RegexError,
    RegexCompileFlags,
    RegexMatchFlags,
    MatchInfo,
    RefCount,
    SList,
    // Shell utilities
    ShellError,
    // Sorted sequence
    Sequence,
    SequenceIter,
    // Relation (deprecated)
    Relation,
    Tuples,
    // RefString
    RefString,
    // String chunk
    StringChunk,
    // String vector builder
    StrvBuilder,
    // Trash stack (deprecated)
    TrashStack,
    Size,
    TraverseFn,
    TraverseFlags,
    TraverseNodeFn,
    TraverseType,
    // Scanner
    Scanner,
    ScannerConfig,
    // Seek
    SeekType,
    // Slice (deprecated)
    SliceConfig,
    // Source
    Source,
    SourceFuncs,
    SourceFlags,
    SourceFunc,
    SourcePrepareFunc,
    SourceCheckFunc,
    SourceDispatchFunc,
    SourceFinalizeFunc,
    SourceCallbackFuncs,
    // Spawn
    SpawnError,
    SpawnFlags,
    SpawnResult,
    SpawnChildSetupFunc,
    SpawnPlatform,
    NoSpawnPlatform,
    Pid,
    // Stdio
    StatBuf,
    OpenFlags,
    StdioPlatform,
    NoStdioPlatform,
    F_OK,
    R_OK,
    W_OK,
    X_OK,
    S_IRWXU,
    S_IRUSR,
    S_IWUSR,
    S_IXUSR,
    S_IRWXG,
    S_IRGRP,
    S_IWGRP,
    S_IXGRP,
    S_IRWXO,
    S_IROTH,
    S_IWOTH,
    S_IXOTH,
    // Test utils
    TestCase,
    TestSuite,
    TestTrapFlags,
    TestTrapStatus,
    TestSubprocessFlags,
    // Thread pool
    ThreadPool,
    ThreadPoolError,
    // Timer
    Timer,
    // Tree
    Tree,
    // Time zone
    TimeZone,
    TimeType,
    // Thread
    GMutex,
    GRecMutex,
    GRWLock,
    GCond,
    Once,
    OnceStatus,
    ThreadError,
    UInt,
    // URI parsing
    Uri,
    UriError,
    UriFlags,
    UriHideFlags,
    // Unicode
    UnicodeType,
    UnicodeBreakType,
    NormalizeMode,
    UnicodeScript,
    // Version
    GLIB_BINARY_AGE,
    GLIB_INTERFACE_AGE,
    GLIB_MAJOR_VERSION,
    GLIB_MICRO_VERSION,
    GLIB_MINOR_VERSION,
    // UTF-8 / Unicode
    Unichar,
    Unichar2,
    // Variant value container
    Variant,
    VariantBuilder,
    VariantParseError,
    VariantClass,
    // Variant types
    VariantType,
    DATE_BAD_JULIAN,
    HOOK_FLAG_ACTIVE,
    HOOK_FLAG_IN_CALL,
    HOOK_FLAG_MASK,
    MEM_ALIGN,
    OPTION_REMAINING,
    VARIANT_TYPE_ANY,
    VARIANT_TYPE_ARRAY,
    VARIANT_TYPE_BASIC,
    VARIANT_TYPE_BOOLEAN,
    VARIANT_TYPE_BYTE,
    VARIANT_TYPE_BYTESTRING,
    VARIANT_TYPE_BYTESTRING_ARRAY,
    VARIANT_TYPE_DICTIONARY,
    VARIANT_TYPE_DICT_ENTRY,
    VARIANT_TYPE_DOUBLE,
    VARIANT_TYPE_HANDLE,
    VARIANT_TYPE_INT16,
    VARIANT_TYPE_INT32,
    VARIANT_TYPE_INT64,
    VARIANT_TYPE_MAYBE,
    VARIANT_TYPE_OBJECT_PATH,
    VARIANT_TYPE_SIGNATURE,
    VARIANT_TYPE_STRING,
    VARIANT_TYPE_STRING_ARRAY,
    VARIANT_TYPE_TUPLE,
    VARIANT_TYPE_UINT16,
    VARIANT_TYPE_UINT32,
    VARIANT_TYPE_UINT64,
    VARIANT_TYPE_UNIT,
    VARIANT_TYPE_VARDICT,
    VARIANT_TYPE_VARIANT,
    // Time span constants
    TIME_SPAN_DAY,
    TIME_SPAN_HOUR,
    TIME_SPAN_MILLISECOND,
    TIME_SPAN_MINUTE,
    TIME_SPAN_SECOND,
    TimeSpan,
    // Scanner token types
    TokenType,
    TokenValue,
    ErrorType,
    // OS info keys
    OS_INFO_KEY_NAME,
    OS_INFO_KEY_PRETTY_NAME,
    OS_INFO_KEY_VERSION,
    OS_INFO_KEY_VERSION_CODENAME,
    OS_INFO_KEY_VERSION_ID,
    OS_INFO_KEY_ID,
    OS_INFO_KEY_HOME_URL,
    OS_INFO_KEY_DOCUMENTATION_URL,
    OS_INFO_KEY_SUPPORT_URL,
    OS_INFO_KEY_BUG_REPORT_URL,
    OS_INFO_KEY_PRIVACY_POLICY_URL,
    // Time constants
    USEC_PER_SEC,
    NSEC_PER_SEC,
    ClockFn,
    // GObject type system
    GType,
    GTypeFlags,
    GTypeFundamentalFlags,
    GTypeInfo,
    GTypeValueTable,
    GValueData,
    TypeClassData,
    TypeInstanceData,
    TypeQuery,
    G_TYPE_INVALID,
    G_TYPE_NONE,
    G_TYPE_INTERFACE,
    G_TYPE_CHAR,
    G_TYPE_UCHAR,
    G_TYPE_BOOLEAN,
    G_TYPE_INT,
    G_TYPE_UINT,
    G_TYPE_LONG,
    G_TYPE_ULONG,
    G_TYPE_INT64,
    G_TYPE_UINT64,
    G_TYPE_ENUM,
    G_TYPE_FLAGS,
    G_TYPE_FLOAT,
    G_TYPE_DOUBLE,
    G_TYPE_STRING,
    G_TYPE_POINTER,
    G_TYPE_BOXED,
    G_TYPE_PARAM,
    G_TYPE_OBJECT,
    G_TYPE_VARIANT,
    // GValue
    GValue,
    TransformFunc,
    // GParamSpec
    ParamSpec,
    ParamID,
    ParamFlags,
    // GSignal
    SignalID,
    HandlerID,
    SignalFlags,
    ConnectFlags,
    SignalCallback,
    SignalQuery,
    // GObject
    GObject,
    ObjectFlags,
    WeakRefCallback,
    PropertyBinding,
};

/// Initialize GLib logging to route through the kernel serial output.
///
/// Call this after the serial port is initialized to get GLib log messages
/// on the serial console instead of being silently dropped.
pub fn init_glib_logging() {
    // Set print handlers that write to serial output
    glib_native::set_print_handler(Some(serial_print_handler));
    glib_native::set_printerr_handler(Some(serial_printerr_handler));
    timer_set_clock(rustos_glib_clock_us);
}

static GLIB_SMOKE_HOOK_COUNT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Exercise a small cross-section of the Rust-native GLib layer.
///
/// This is intended for boot-time integration validation after the kernel heap
/// is initialized. It verifies that converted GLib primitives are linked and
/// usable in RustOS without pulling in the original C GLib runtime.
pub fn smoke_check() -> Result<(), &'static str> {
    if checked_add_size(1, 2) != Some(3) {
        return Err("checked size arithmetic");
    }

    if checked_mul_u32(u32::MAX, 2).is_some() {
        return Err("checked u32 overflow");
    }

    if base64_encode(b"RustOS") != "UnVzdE9T" {
        return Err("base64 encode");
    }

    if base64_decode("UnVzdE9T").as_slice() != b"RustOS" {
        return Err("base64 decode");
    }

    let bytes = Bytes::new(b"glib");
    if bytes.len() != 4 || bytes.data() != b"glib" {
        return Err("GBytes");
    }

    if checksum_type_get_length(ChecksumType::Sha256) != Some(32) {
        return Err("checksum type length");
    }

    if compute_checksum_for_data(ChecksumType::Sha256, b"glib").len() != 64 {
        return Err("checksum data");
    }

    let network_order = g_htonl(0x0102_0304);
    if g_ntohl(network_order) != 0x0102_0304 {
        return Err("network byte order");
    }

    if get_charset() != "UTF-8" || get_console_charset() != "UTF-8" || get_codeset() != "UTF-8" {
        return Err("charset defaults");
    }

    if build_filename(&["/usr/", "/bin", "glib"]) != "/usr/bin/glib" {
        return Err("filename builder");
    }

    let uri = filename_to_uri("/rustos/glib", None).map_err(|_| "filename to uri")?;
    let (path, host) = filename_from_uri(&uri).map_err(|_| "filename from uri")?;
    if path != "/rustos/glib" || host.is_some() {
        return Err("file URI roundtrip");
    }

    let quark = quark_from_static_string(Some("rustos-glib"));
    if quark == 0
        || quark_try_string(Some("rustos-glib")) != quark
        || quark_to_string(quark) != Some("rustos-glib")
    {
        return Err("quark intern");
    }

    let mut ref_count = RefCount::new();
    ref_count.inc();
    if !ref_count.compare(2) || ref_count.dec() || !ref_count.dec() {
        return Err("refcount");
    }

    if !str_has_prefix("rustos-glib", "rustos") || !str_has_suffix("rustos-glib", "glib") {
        return Err("string predicates");
    }

    let mut string = GString::new(Some("glib"));
    string.append("-native");
    if string.as_str() != "glib-native" {
        return Err("GString append");
    }

    let byte_array = ByteArray::new();
    byte_array.append(b"glib", 4);
    if byte_array.len() != 4 || byte_array.data() != b"glib" {
        return Err("GByteArray");
    }

    if !pattern_match_simple("glib-*", "glib-native") {
        return Err("pattern match");
    }

    let quoted = shell_quote("glib native");
    if shell_unquote(&quoted).map_err(|_| "shell unquote")? != "glib native" {
        return Err("shell quote roundtrip");
    }

    let argv = shell_parse_argv("glib --native 'rust os'").map_err(|_| "shell argv")?;
    if argv.len() != 3 || argv[0] != "glib" || argv[1] != "--native" || argv[2] != "rust os" {
        return Err("shell argv");
    }

    if !type_string_is_valid("as") || type_string_is_valid("glib") {
        return Err("variant type validation");
    }

    let variant_type = VariantType::new("as").ok_or("variant type")?;
    if !variant_type.is_array()
        || variant_type.element().map(|element| element.as_str() != "s").unwrap_or(true)
    {
        return Err("variant type element");
    }

    if !type_equal("(is)", "(is)") || type_hash("s") == type_hash("i") {
        return Err("variant type hash");
    }

    let mut queue = GQueue::new();
    queue.push_tail("glib");
    queue.push_tail("native");
    if queue.get_length() != 2 || queue.pop_head() != Some("glib") || queue.pop_head() != Some("native") {
        return Err("GQueue");
    }

    if compute_hmac_for_string(
        ChecksumType::Sha1,
        b"key",
        "The quick brown fox jumps over the lazy dog",
    ) != "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9"
    {
        return Err("HMAC");
    }

    if !is_leap_year(2024)
        || get_days_in_month(DateMonth::February, 2024) != 29
        || valid_dmy(31, DateMonth::April, 2024)
    {
        return Err("date helpers");
    }

    let date = Date::new_dmy(27, DateMonth::June, 2026);
    if !date.valid()
        || date.day() != 27
        || date.month() != DateMonth::June
        || date.year() != 2026
    {
        return Err("GDate");
    }

    if hostname_is_non_ascii("example.com") || !hostname_is_ascii_encoded("xn--rustos.local") {
        return Err("hostname classification");
    }

    if !hostname_is_ip_address("192.168.1.1") || hostname_is_ip_address("192.168.01.1") {
        return Err("hostname IP detection");
    }

    if hostname_to_ascii("Example.COM").as_deref() != Some("example.com")
        || hostname_to_unicode("example.com").as_deref() != Some("example.com")
    {
        return Err("hostname conversion");
    }

    let envp = environ_setenv(alloc::vec::Vec::new(), "GLIB", "native", true);
    if environ_getenv(&envp, "GLIB").as_deref() != Some("native") {
        return Err("environ getenv");
    }

    let envp = environ_unsetenv(envp, "GLIB");
    if environ_getenv(&envp, "GLIB").is_some() {
        return Err("environ unsetenv");
    }

    if strlen("glib") != 4 || strcmp("glib", "glib") != 0 || strcmp("glib", "rust") >= 0 {
        return Err("string compare");
    }

    if !ascii_isalpha(b'G')
        || !ascii_isdigit(b'7')
        || !ascii_isxdigit(b'f')
        || ascii_digit_value(b'7') != 7
        || ascii_xdigit_value(b'f') != 15
        || ascii_tolower(b'G') != b'g'
        || ascii_toupper(b'g') != b'G'
        || ascii_strcasecmp("GLib", "glib") != 0
    {
        return Err("ASCII helpers");
    }

    if strjoin(Some("-"), &["glib", "native"]) != "glib-native" {
        return Err("string join");
    }

    let split = strsplit("glib:native:rustos", ":", 0);
    if split.len() != 3 || split[0] != "glib" || split[1] != "native" || split[2] != "rustos" {
        return Err("string split");
    }

    if !strv_contains(&["glib", "native"], "native")
        || !strv_equal(&["glib", "native"], &["glib", "native"])
        || strv_length(&["glib", "native"]) != 2
        || strdupv(&["glib", "native"]).len() != 2
    {
        return Err("string vector");
    }

    let mut strv_builder = StrvBuilder::new();
    strv_builder.add("glib");
    strv_builder.addv(&["native", "rustos"]);
    let strv = strv_builder.into_vec();
    if strv.len() != 3 || strv[0] != "glib" || strv[1] != "native" || strv[2] != "rustos" {
        return Err("GStrvBuilder");
    }

    if swap_u16_le_be(0x1234) != 0x3412
        || swap_u32_le_be(0x0102_0304) != 0x0403_0201
        || swap_u64_le_be(0x0102_0304_0506_0708) != 0x0807_0605_0403_0201
    {
        return Err("byte swap");
    }

    if !path_is_absolute("/usr/bin/glib")
        || path_skip_root("/usr/bin/glib") != Some("usr/bin/glib")
        || path_get_basename("/usr/bin/glib") != "glib"
        || path_get_dirname("/usr/bin/glib") != "/usr/bin"
    {
        return Err("path helpers");
    }

    let e_acute = b"\xC3\xA9";
    if !utf8_validate(e_acute)
        || utf8_len(e_acute) != Some(2)
        || utf8_get_char(e_acute) != Some((0xE9, 2))
        || unichar_to_utf8_string(0xE9).as_deref() != Some("\u{e9}")
    {
        return Err("UTF-8 helpers");
    }

    if !unichar_isalpha('G' as u32)
        || !unichar_isdigit('7' as u32)
        || unichar_digit_value('7' as u32) != 7
        || unichar_toupper('g' as u32) != 'G' as u32
        || unichar_tolower('G' as u32) != 'g' as u32
    {
        return Err("Unichar helpers");
    }

    let array = GArray::new(false, false, 1);
    array.append_vals(Some(b"glib"), 4);
    let array_data = array.data();
    if array.len() != 4 || array_data.as_slice() != b"glib" {
        return Err("GArray");
    }

    let ptr_array = PtrArray::new();
    let first_ptr = core::ptr::null_mut();
    let second_ptr = core::ptr::NonNull::<()>::dangling().as_ptr();
    ptr_array.add(first_ptr);
    ptr_array.add(second_ptr);
    if ptr_array.len() != 2
        || ptr_array.remove_index(0) != Some(first_ptr)
        || ptr_array.remove_index(0) != Some(second_ptr)
        || !ptr_array.is_empty()
    {
        return Err("GPtrArray");
    }

    let hash_table = HashTable::new(None, None);
    let mut hash_key = 0usize;
    let mut hash_value = 0usize;
    let hash_key_ptr = (&mut hash_key as *mut usize).cast::<()>();
    let hash_value_ptr = (&mut hash_value as *mut usize).cast::<()>();
    if !hash_table.insert(hash_key_ptr, hash_value_ptr)
        || hash_table.size() != 1
        || !hash_table.contains(hash_key_ptr)
        || hash_table.lookup(hash_key_ptr) != hash_value_ptr
    {
        return Err("GHashTable");
    }

    let mut key_file = KeyFile::new();
    key_file
        .load_from_data("[RustOS]\nname=glib-native\nversion=1\n", KeyFileFlags::NONE)
        .map_err(|_| "GKeyFile load")?;
    if key_file.get_string("RustOS", "name").map_err(|_| "GKeyFile string")? != "glib-native"
        || key_file.get_integer("RustOS", "version").map_err(|_| "GKeyFile integer")? != 1
    {
        return Err("GKeyFile read");
    }

    key_file.set_string("RustOS", "status", "wired");
    if key_file.get_string("RustOS", "status").map_err(|_| "GKeyFile set")? != "wired" {
        return Err("GKeyFile set");
    }

    let uri = Uri::parse("http://example.com:8080/rustos?glib=1#native", UriFlags::NONE)
        .map_err(|_| "GUri parse")?;
    if uri.scheme() != "http"
        || uri.host() != "example.com"
        || uri.port() != Some(8080)
        || uri.path() != "/rustos"
        || uri.query() != Some("glib=1")
        || uri.fragment() != Some("native")
    {
        return Err("GUri fields");
    }

    let built_uri = Uri::build(
        UriFlags::NONE,
        "https",
        None,
        "rustos.local",
        Some(443),
        "/glib",
        Some("native=1"),
        None,
    );
    if built_uri.to_string() != "https://rustos.local:443/glib?native=1" {
        return Err("GUri build");
    }

    if join(
        UriFlags::NONE,
        "rustos",
        None,
        "",
        None,
        "/glib/native",
        None,
        None,
    ) != "rustos:/glib/native"
    {
        return Err("GUri join");
    }

    if check_version(GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION, GLIB_MICRO_VERSION).is_some()
        || !check_version_bool(GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION, GLIB_MICRO_VERSION)
        || check_version_bool(GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION + 1, 0)
    {
        return Err("GLib version");
    }

    let markup_parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
    let markup = markup_parser
        .parse(r#"<root attr="value">text</root>"#)
        .map_err(|_| "GMarkup parse")?;
    if markup.name != "root" || markup.attributes.len() != 1 || markup.attributes[0].name != "attr" {
        return Err("GMarkup element");
    }
    match markup.children.first() {
        Some(MarkupNode::Text(text)) if text == "text" => {}
        _ => return Err("GMarkup text"),
    }

    let mut string_chunk = StringChunk::new(64);
    let chunk_idx = string_chunk.insert("glib");
    let const_idx = string_chunk.insert_const("native");
    if string_chunk.len() != 2
        || string_chunk.get(chunk_idx) != Some("glib")
        || string_chunk.get(const_idx) != Some("native")
    {
        return Err("GStringChunk");
    }
    string_chunk.clear();
    if !string_chunk.is_empty() {
        return Err("GStringChunk clear");
    }

    let mut tree = NTree::new();
    let root = tree.new_root(1);
    let child_a = tree.append(root, 2);
    tree.append(root, 3);
    tree.append(child_a, 4);
    if tree.n_children(root) != 2
        || tree.depth(child_a) != 2
        || tree.n_nodes(root, TraverseFlags::ALL) != 4
        || tree.find(root, TraverseType::PreOrder, TraverseFlags::ALL, |data| *data == 4).is_none()
    {
        return Err("GNode tree");
    }

    let error_domain = file_error_quark();
    let error = error_new_literal(error_domain, 7, "glib error");
    let copied_error = error_copy(&error);
    if error.domain() != error_domain
        || error.code() != 7
        || error.message() != "glib error"
        || !error_matches(&copied_error, error_domain, 7)
    {
        return Err("GError");
    }

    let mut error_slot: Option<Error> = None;
    set_error_literal(Some(&mut error_slot), error_domain, 9, "set error");
    if error_slot.as_ref().map(|err| err.message()) != Some("set error") {
        return Err("GError set");
    }
    clear_error(Some(&mut error_slot));
    if error_slot.is_some() {
        return Err("GError clear");
    }

    let mut option_value = alloc::string::String::new();
    let option_entries = [OptionEntry {
        long_name: Some("glib"),
        short_name: '\0',
        flags: 0,
        arg: OptionArg::String,
        arg_data: (&mut option_value as *mut alloc::string::String).cast::<core::ffi::c_void>(),
        description: Some("GLib mode"),
        arg_description: Some("MODE"),
    }];
    let mut option_context = option_context_new(None);
    option_context.set_help_enabled(false);
    option_context.add_main_entries(&option_entries, None);
    let mut option_argv = alloc::vec![
        alloc::string::String::from("rustos"),
        alloc::string::String::from("--glib=native"),
    ];
    option_context.parse(&mut option_argv).map_err(|_| "GOption parse")?;
    if option_value != "native" || option_argv != [alloc::string::String::from("rustos")] {
        return Err("GOption");
    }

    let async_queue = AsyncQueue::new();
    async_queue.push(2);
    async_queue.push_front(1);
    if async_queue.len() != 2
        || async_queue.try_pop() != Some(1)
        || async_queue.try_pop_back() != Some(2)
        || async_queue.try_pop().is_some()
    {
        return Err("GAsyncQueue");
    }

    let mut sequence = Sequence::new();
    sequence.insert_sorted(30, |a, b| a.cmp(b));
    sequence.insert_sorted(10, |a, b| a.cmp(b));
    sequence.insert_sorted(20, |a, b| a.cmp(b));
    if sequence.len() != 3
        || sequence.get(0) != Some(&10)
        || sequence.get(1) != Some(&20)
        || sequence.lookup(&30, |a, b| a.cmp(b)).is_none()
    {
        return Err("GSequence");
    }

    GLIB_SMOKE_HOOK_COUNT.store(0, core::sync::atomic::Ordering::SeqCst);
    let mut hooks = HookList::new();
    let hook_id = hooks.add(glib_smoke_hook, 3);
    if hook_id == 0 || hooks.find_data(true, 3) != Some(hook_id) {
        return Err("GHook find");
    }
    hooks.invoke(false);
    if GLIB_SMOKE_HOOK_COUNT.load(core::sync::atomic::Ordering::SeqCst) != 3 {
        return Err("GHook invoke");
    }
    if !hooks.destroy(hook_id) || hooks.find_data(true, 3).is_some() {
        return Err("GHook destroy");
    }

    let atomic_int = AtomicInt::new(1);
    atomic_int.inc();
    if atomic_int.get() != 2
        || !atomic_int.compare_and_exchange(2, 4)
        || atomic_int.get() != 4
        || atomic_int.dec_and_test()
    {
        return Err("GAtomicInt");
    }

    let atomic_uint = AtomicUInt::new(2);
    atomic_uint.inc();
    if atomic_uint.get() != 3
        || atomic_uint.exchange(5) != 3
        || atomic_uint.get() != 5
        || !atomic_uint.compare_and_exchange(5, 1)
        || !atomic_uint.dec_and_test()
    {
        return Err("GAtomicUInt");
    }

    let datalist = DataList::new();
    let datalist_key = quark_from_static_string(Some("rustos-datalist"));
    let mut datalist_value = 0usize;
    let datalist_ptr = (&mut datalist_value as *mut usize).cast::<core::ffi::c_void>();
    datalist_id_set_data(&datalist, datalist_key, datalist_ptr);
    if datalist_id_get_data(&datalist, datalist_key) != datalist_ptr {
        return Err("GDataList get");
    }
    if datalist_id_remove_no_notify(&datalist, datalist_key) != datalist_ptr
        || !datalist_id_get_data(&datalist, datalist_key).is_null()
    {
        return Err("GDataList remove");
    }

    let variant = Variant::new_tuple(alloc::vec![
        Variant::new_string("glib"),
        Variant::new_int32(42),
    ]);
    if variant.classify() != VariantClass::Tuple
        || variant.n_children() != 2
        || variant
            .get_child_value(0)
            .map(|child| child.get_string() != "glib")
            .unwrap_or(true)
        || variant.get_child_value(1).map(|child| child.get_int32()) != Some(42)
    {
        return Err("GVariant tuple");
    }

    let datetime = DateTime::from_unix_utc(0);
    if datetime.year() != 1970
        || datetime.month() != 1
        || datetime.day_of_month() != 1
        || datetime.hour() != 0
        || datetime.minute() != 0
        || datetime.second() != 0
        || datetime.to_unix() != 0
    {
        return Err("GDateTime epoch");
    }

    let next_day = DateTime::from_unix_utc(86_400);
    if datetime.difference(&next_day) != TIME_SPAN_DAY {
        return Err("GDateTime difference");
    }

    let mut timer = Timer::new();
    if !timer.is_active() {
        return Err("GTimer active");
    }
    let (_, elapsed_us) = timer.elapsed();
    if elapsed_us >= 1_000_000 {
        return Err("GTimer elapsed");
    }
    timer.stop();
    if timer.is_active() {
        return Err("GTimer stop");
    }

    let timezone = TimeZone::new_offset(3600);
    let mut timezone_time = 0i64;
    if timezone.identifier() != "+01:00"
        || timezone.offset(0) != 3600
        || timezone.adjust_time(TimeType::Universal, &mut timezone_time) != 0
        || timezone_time != 3600
    {
        return Err("GTimeZone");
    }

    let mut scanner = Scanner::new(ScannerConfig::default());
    scanner.input_text("glib 42");
    if scanner.get_next_token() != TokenType::Identifier {
        return Err("GScanner identifier");
    }
    match scanner.cur_value() {
        TokenValue::Identifier(value) if value == "glib" => {}
        _ => return Err("GScanner identifier value"),
    }
    if scanner.get_next_token() != TokenType::Int {
        return Err("GScanner int");
    }
    match scanner.cur_value() {
        TokenValue::Int(42) => {}
        _ => return Err("GScanner int value"),
    }

    let mut checksum = Checksum::new(ChecksumType::Md5);
    checksum.update(b"abc");
    let mut checksum_digest = [0u8; 16];
    if checksum.get_digest(&mut checksum_digest) != 16
        || checksum.get_string() != "900150983cd24fb0d6963f7d28e17f72"
    {
        return Err("GChecksum");
    }

    let mut hmac = Hmac::new(ChecksumType::Sha1, b"key");
    hmac.update(b"The quick brown fox jumps over the lazy dog");
    if hmac.get_string() != "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9" {
        return Err("GHmac");
    }

    let bitlock_word = core::sync::atomic::AtomicI32::new(0);
    if !bit_trylock(&bitlock_word, 0) || bit_trylock(&bitlock_word, 0) {
        return Err("GBitLock");
    }
    bit_unlock(&bitlock_word, 0);
    if !bit_trylock(&bitlock_word, 0) {
        return Err("GBitLock unlock");
    }
    bit_unlock(&bitlock_word, 0);

    let pointer_bitlock_word = core::sync::atomic::AtomicUsize::new(0);
    if !pointer_bit_trylock(&pointer_bitlock_word, 0)
        || pointer_bit_trylock(&pointer_bitlock_word, 0)
    {
        return Err("GPointerBitLock");
    }
    pointer_bit_unlock(&pointer_bitlock_word, 0);

    // Regex engine
    let re = Regex::new("g(\\w+)b", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT)
        .map_err(|_| "GRegex compile")?;
    let info = re.r#match("glib", RegexMatchFlags::DEFAULT);
    if !info.matches() || info.fetch(0) != Some("glib".to_owned()) || info.fetch(1) != Some("li".to_owned()) {
        return Err("GRegex match");
    }
    if !Regex::match_simple("\\d+", "abc123", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT) {
        return Err("GRegex match_simple");
    }
    let re2 = Regex::new("\\s+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT)
        .map_err(|_| "GRegex compile 2")?;
    let parts = re2.split("glib native rustos", RegexMatchFlags::DEFAULT);
    if parts.len() != 3 || parts[0] != "glib" || parts[1] != "native" || parts[2] != "rustos" {
        return Err("GRegex split");
    }
    let re3 = Regex::new("(\\w+)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT)
        .map_err(|_| "GRegex compile 3")?;
    let replaced = re3.replace("glib native", "[$1]", RegexMatchFlags::DEFAULT);
    if replaced != "[glib] [native]" {
        return Err("GRegex replace");
    }

    // Thread pool (inline execution, no OS threads)
    let pool = ThreadPool::new(glib_threadpool_noop, 1, false).map_err(|_| "GThreadPool new")?;
    pool.push(1).map_err(|_| "GThreadPool push 1")?;
    pool.push(2).map_err(|_| "GThreadPool push 2")?;
    pool.push(3).map_err(|_| "GThreadPool push 3")?;
    if pool.unprocessed() != 3 {
        return Err("GThreadPool unprocessed");
    }

    // Test utils
    let mut suite = TestSuite::new("rustos-glib");
    suite.add(TestCase::new("regex", glib_test_noop));
    suite.add(TestCase::new("threadpool", glib_test_noop));
    if suite.count() != 2 {
        return Err("GTestSuite");
    }

    // GType system
    type_init();
    if type_from_name("gint") != G_TYPE_INT || type_from_name("GObject") != G_TYPE_OBJECT {
        return Err("GType fundamental lookup");
    }
    let info = GTypeInfo {
        class_size: 64,
        instance_size: 32,
        class_init: None,
        instance_init: None,
        value_table: None,
    };
    let custom_type = type_register_static(G_TYPE_OBJECT, "RustOSObject", &info, GTypeFlags::NONE);
    if custom_type == G_TYPE_INVALID || type_name(custom_type) != Some("RustOSObject".to_owned()) {
        return Err("GType register static");
    }
    if !type_is_a(custom_type, G_TYPE_OBJECT) || type_depth(custom_type) != 2 {
        return Err("GType hierarchy");
    }

    // GValue
    let mut int_val = GValue::for_type(G_TYPE_INT);
    int_val.set_int(42);
    if int_val.get_int() != 42 || !int_val.holds(G_TYPE_INT) {
        return Err("GValue int");
    }
    let str_val = value_new_string("rustos");
    if str_val.get_string() != Some("rustos") {
        return Err("GValue string");
    }

    // GSignal
    let sig_id = signal_new("test-changed", G_TYPE_OBJECT, SignalFlags::RUN_LAST, G_TYPE_NONE, &[]);
    if sig_id == 0 || signal_lookup("test-changed", G_TYPE_OBJECT) != sig_id {
        return Err("GSignal register");
    }
    let query = signal_query(sig_id);
    if query.is_none() || query.unwrap().signal_name != "test-changed" {
        return Err("GSignal query");
    }

    // GObject
    let obj = object_new(G_TYPE_OBJECT);
    if obj.ref_count() != 1 || obj.type_name() != "GObject" {
        return Err("GObject basic");
    }
    obj.ref_();
    if obj.ref_count() != 2 {
        return Err("GObject ref");
    }
    obj.unref();
    if obj.ref_count() != 1 {
        return Err("GObject unref");
    }

    // GObject properties
    obj.install_properties(vec![
        ParamSpec::int("x", "x", "x coord", 0, 1000, 0, ParamFlags::READWRITE),
        ParamSpec::string("name", "n", "name", "", ParamFlags::READWRITE),
    ]);
    obj.set_property("x", value_new_int(77));
    if obj.get_property("x").map(|v| v.get_int()) != Some(77) {
        return Err("GObject property set");
    }
    obj.set_property("name", value_new_string("test-obj"));
    if obj.get_property("name").map(|v| v.get_string().map(ToOwned::to_owned)) != Some(Some("test-obj".to_owned())) {
        return Err("GObject property name");
    }

    // GModule — the kernel has no dynamic loader, so we exercise the
    // unsupported-platform paths plus the path-building and error-quark
    // helpers. The behavior matches upstream gmodule.c when
    // G_MODULE_IMPL is undefined: every operation fails with the
    // "dynamic modules are not supported by this system" string, but the
    // API surface is linkable and the registry logic is exercised.
    if module_supported::<NoModulePlatform>() {
        return Err("GModule supported on no-platform");
    }
    if module_error_quark() == 0 {
        return Err("GModule error quark");
    }
    let built = module_build_path::<NoModulePlatform>(Some("/lib"), "rustos");
    if built != "/lib/librustos.so" {
        return Err("GModule build_path");
    }
    let built_no_dir = module_build_path::<NoModulePlatform>(None, "rustos");
    if built_no_dir != "librustos.so" {
        return Err("GModule build_path no dir");
    }
    let built_lib_prefix = module_build_path::<NoModulePlatform>(Some("/lib"), "libfoo");
    if built_lib_prefix != "/lib/libfoo" {
        return Err("GModule build_path lib prefix");
    }
    let open_result = module_open_full::<NoModulePlatform>(Some("rustos.so"), GModuleFlags::NONE);
    if open_result.is_ok() {
        return Err("GModule open on no-platform should fail");
    }
    let (err_code, err_msg) = open_result.unwrap_err();
    if err_code != GModuleError::Failed
        || !err_msg.contains("not supported")
        || module_error().as_deref().map(|s| !s.contains("not supported")).unwrap_or(true)
    {
        return Err("GModule open error path");
    }
    let main_open_result = module_open::<NoModulePlatform>(None, GModuleFlags::BIND_LAZY);
    if main_open_result.is_ok() || main_open_result.unwrap_err().0 != GModuleError::Failed {
        return Err("GModule main open error path");
    }
    // Construct a transient GModule directly to exercise name/ref_count/
    // make_resident without going through the platform (which is
    // unsupported). We don't add it to the registry; we just validate the
    // GModule struct's own behavior.
    let transient = glib_native::gmodule::GModule::new(
        Some("/lib/librustos.so".to_owned()),
        core::ptr::null_mut(),
    );
    if transient.name() != "/lib/librustos.so" || transient.ref_count() != 1 || transient.is_resident() {
        return Err("GModule struct fields");
    }
    transient.make_resident();
    if !transient.is_resident() {
        return Err("GModule make_resident");
    }
    let transient_main = glib_native::gmodule::GModule::new(None, core::ptr::null_mut());
    if transient_main.name() != "main" {
        return Err("GModule main name");
    }
    // module_symbol on the unsupported platform should record an error.
    let sym_result = module_symbol::<NoModulePlatform>(&transient, "rustos_init");
    if sym_result.is_ok() || sym_result.unwrap_err().0 != GModuleError::Failed {
        return Err("GModule symbol error path");
    }
    // module_close on the unsupported platform should also fail.
    let close_result = module_close::<NoModulePlatform>(&transient);
    if close_result.is_ok() || close_result.unwrap_err().0 != GModuleError::Failed {
        return Err("GModule close error path");
    }

    Ok(())
}

fn glib_smoke_hook(data: usize) {
    GLIB_SMOKE_HOOK_COUNT.fetch_add(data, core::sync::atomic::Ordering::SeqCst);
}

fn glib_threadpool_noop(_data: usize) {}

fn glib_test_noop() {}

fn rustos_glib_clock_us() -> i64 {
    crate::time::uptime_us().min(i64::MAX as u64) as i64
}

fn serial_print_handler(string: &str) {
    for byte in string.bytes() {
        // SAFETY: Raw I/O to COM1 for logging. See docs/SAFETY.md#io-port-access.
        unsafe {
            crate::early_serial_write_byte(byte);
        }
    }
}

fn serial_printerr_handler(string: &str) {
    for byte in string.bytes() {
        // SAFETY: Raw I/O to COM1 for logging. See docs/SAFETY.md#io-port-access.
        unsafe {
            crate::early_serial_write_byte(byte);
        }
    }
}
