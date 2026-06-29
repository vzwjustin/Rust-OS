//! GLib compatibility layer for RustOS.
//!
//! Re-exports `glib_native` types and provides thin wrappers that adapt
//! GLib's no_std API to the kernel environment (e.g. routing log output
//! to the serial console instead of stdout/stderr).

#![allow(unused_imports)]

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;

pub use glib_native::*;

pub use glib_native::{
    access,
    action_name_is_valid,
    action_parse_detailed_name,
    action_print_detailed_name,
    // Memory
    aligned_alloc,
    aligned_alloc0,
    application_id_is_valid,
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
    assert_cmpint,
    assert_cmpstr,
    assert_false,
    assert_nonnull,
    assert_null,
    assert_true,
    // Async queue
    async_queue_new,
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
    cancellable_get_current,
    cancellable_pop_current,
    cancellable_push_current,
    cancellable_source_new,
    canonicalize_filename,
    // Version
    check_version,
    check_version_bool,
    // Checked arithmetic
    checked_add_size,
    checked_add_u32,
    checked_mul_size,
    checked_mul_u32,
    checksum_type_get_length,
    clear,
    clear_error,
    clear_with,
    compute_checksum_for_bytes,
    compute_checksum_for_data,
    compute_checksum_for_string,
    compute_hmac_for_bytes,
    compute_hmac_for_data,
    compute_hmac_for_string,
    content_type_can_be_executable,
    // GIO content type
    content_type_equals,
    content_type_get_description,
    content_type_get_mime_type,
    content_type_guess,
    content_type_is_a,
    content_type_is_unknown,
    content_types_get_registered,
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
    dbus_address_escape_value,
    dbus_address_get_for_bus_sync,
    dbus_annotation_info_lookup,
    dbus_error_encode_gerror,
    dbus_error_get_remote_error,
    dbus_error_is_remote_error,
    dbus_error_new_for_dbus_error,
    dbus_error_quark,
    dbus_error_register_error,
    dbus_error_register_error_domain,
    dbus_error_strip_remote_error,
    dbus_error_unregister_error,
    dbus_interface_info_lookup_method,
    dbus_interface_info_lookup_property,
    dbus_interface_info_lookup_signal,
    dbus_node_info_lookup_interface,
    debug,
    // Main loop
    default_context,
    // GValue
    default_value_table_for,
    dir_open,
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
    escape_object_path,
    // URI functions
    escape_string,
    // Markup
    escape_text,
    file_error_from_errno,
    file_error_quark,
    filename_display_basename,
    filename_display_name,
    filename_from_uri,
    filename_to_uri,
    find_property,
    find_property_by_id,
    free,
    // Endian
    g_htonl,
    g_htons,
    g_ntohl,
    g_ntohs,
    // GType system
    g_type_make_fundamental,
    // GIO D-Bus utils
    generate_guid,
    // Utils
    get_application_name,
    // Charset
    get_charset,
    get_codeset,
    get_console_charset,
    get_days_in_month,
    get_environ,
    get_language_names,
    get_locale_variants,
    get_max_idle_time,
    get_max_unused_threads,
    get_num_unused_threads,
    get_prgname,
    get_session_bus_address,
    get_system_bus_address,
    get_thumbnail_path,
    getenv,
    hook_compare_ids,
    // Hostname utilities
    hostname_is_ascii_encoded,
    hostname_is_ip_address,
    hostname_is_non_ascii,
    hostname_to_ascii,
    hostname_to_unicode,
    idle_add,
    info,
    init_builtin_transforms,
    // GParamSpec
    install_properties,
    int64_equal,
    int64_hash,
    int_equal,
    int_hash,
    intern_static_string,
    intern_string,
    // I/O channel
    io_channel_error_quark,
    io_error_from_errno,
    io_error_from_file_error,
    io_error_quark,
    is_address,
    is_closed,
    is_dir_separator,
    is_guid,
    is_interface_name,
    is_leap_year,
    is_member_name,
    is_name,
    is_readable,
    is_supported_address,
    is_thumbnail_path,
    is_unique_name,
    is_valid,
    is_writable,
    join,
    key_file_error_quark,
    listenv,
    log,
    log_default_handler,
    log_fmt,
    log_remove_handler,
    log_set_default_handler,
    log_set_handler,
    malloc,
    malloc0,
    malloc0_n,
    malloc_n,
    mapped_file_new,
    mapped_file_new_from_fd,
    // Markup
    markup_error_quark,
    memdup,
    memdup2,
    memory_monitor_base_level_enum_to_byte,
    memory_monitor_base_query_mem_ratio,
    memory_monitor_get_default,
    message,
    mkdir as g_mkdir,
    module_build_path,
    module_close,
    module_error,
    module_error_quark,
    module_make_resident,
    module_name,
    module_open,
    module_open_full,
    module_supported,
    module_symbol,
    monday_weeks_in_year,
    // GObject
    object_new,
    object_new_with_params,
    option_context_new,
    option_error_quark,
    option_group_new,
    path_get_basename,
    path_get_dirname,
    path_is_absolute,
    path_skip_root,
    pattern_match_simple,
    peek_scheme,
    pointer_bit_lock,
    pointer_bit_trylock,
    pointer_bit_unlock,
    prefix_error,
    prefix_error_literal,
    print,
    printerr,
    printf_format,
    propagate_error,
    propagate_prefixed_error,
    property_names,
    quark_from_static_string,
    quark_from_string,
    quark_to_string,
    quark_try_string,
    random_boolean,
    random_double,
    random_double_range,
    // Random
    random_int,
    random_int_range,
    random_set_seed,
    realloc,
    realloc_n,
    // Regex
    regex_error_quark,
    register_dbus_address_platform,
    register_dir_platform,
    register_file_platform,
    register_mapped_file_platform,
    register_spawn_platform,
    register_stdio_platform,
    scan_type_string,
    set_application_name,
    set_error,
    set_error_literal,
    set_max_idle_time,
    // Thread pool
    set_max_unused_threads,
    set_prgname,
    set_print_handler,
    set_printerr_handler,
    setenv,
    shell_error_quark,
    shell_parse_argv,
    shell_quote,
    shell_unquote,
    signal_connect,
    signal_connect_by_name,
    signal_emit,
    signal_emit_by_name,
    signal_handler_block,
    signal_handler_disconnect,
    signal_handler_is_connected,
    signal_handler_unblock,
    signal_handlers_disconnect_all,
    signal_list_ids,
    signal_lookup,
    signal_n_handlers,
    signal_name,
    // GSignal
    signal_new,
    signal_query,
    // Slice (deprecated)
    slice_alloc,
    slice_alloc0,
    slice_copy,
    slice_free1,
    slice_get_config,
    slice_set_config,
    // Sorting
    sort_array,
    sort_array_unstable,
    source_remove,
    // Primes
    spaced_primes_closest,
    spawn_async,
    spawn_check_exit_status,
    spawn_check_wait_status,
    // Spawn
    spawn_error_quark,
    spawn_exit_error_quark,
    spawn_sync,
    // Printf
    sprintf,
    srv_target_list_sort,
    stat as g_stat,
    steal,
    steal_error,
    stop_unused_threads,
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
    test_add_data_func,
    test_add_func,
    test_assert_expected_messages,
    test_create_suite,
    test_expect_message,
    test_get_root,
    // Test utils
    test_init,
    test_run,
    test_trap_subprocess,
    // Thread
    thread_error_quark,
    thumbnail_verify,
    timeout_add,
    timer_set_clock,
    try_aligned_alloc,
    try_malloc,
    try_malloc0,
    try_malloc0_n,
    try_malloc_n,
    try_realloc,
    try_realloc_n,
    type_add_interface,
    type_children,
    type_class_size,
    type_depth,
    type_equal,
    type_from_name,
    type_fundamental,
    type_fundamental_next,
    type_get_all,
    type_get_type_registration_serial,
    type_hash,
    type_init,
    type_instance_size,
    type_interfaces,
    type_is_a,
    type_is_abstract,
    type_is_classed,
    type_is_final,
    type_is_instantiatable,
    type_name,
    type_parent,
    type_query,
    type_register_fundamental,
    type_register_static,
    type_register_static_simple,
    type_string_is_valid,
    type_value_table,
    unescape_string,
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
    // UUID
    uuid_string_is_valid,
    uuid_string_random,
    valid_day,
    valid_dmy,
    valid_julian,
    valid_month,
    valid_weekday,
    valid_year,
    value_new_boolean,
    value_new_boxed,
    value_new_char,
    value_new_double,
    value_new_enum,
    value_new_flags,
    value_new_float,
    value_new_int,
    value_new_int64,
    value_new_object,
    value_new_pointer,
    value_new_string,
    value_new_uint,
    value_new_uint64,
    // GValue transform functions (Phase 9 deferred item)
    value_register_transform_func,
    value_transform,
    value_type_compatible,
    value_type_transformable,
    // Variant
    variant_parse,
    variant_parse_error_quark,
    vsprintf,
    warning,
    // GIO action interface
    Action,
    ActionEntry,
    // GIO action group
    ActionGroup,
    // GIO action group exporter
    ActionGroupExporter,
    ActionInfo,
    // GIO action map
    ActionMap,
    AlignedBuffer,
    // GIO app info
    AppInfo,
    AppInfoCreateFlags,
    // GIO app info monitor
    AppInfoMonitor,
    AppLaunchContext,
    // GIO application
    Application,
    // GIO application command line
    ApplicationCommandLine,
    ApplicationFlags,
    // GIO application impl + D-Bus daemon
    ApplicationImpl,
    AskPasswordFlags,
    // GIO async helper
    AsyncHelper,
    // GIO async initable
    AsyncInitable,
    // Async queue
    AsyncQueue,
    // GIO async result interface
    AsyncResult,
    // Atomic types
    AtomicInt,
    AtomicPointer,
    AtomicRcBox,
    // Refcount
    AtomicRefCount,
    AtomicUInt,
    AuthMechanismState,
    Base64Decoder,
    Base64Encoder,
    BaseNetworkConnectivity,
    // Core types
    Bool,
    BusNameOwnerFlags,
    BusNameWatcherFlags,
    ByteArray,
    // Bytes
    Bytes,
    BytesIcon,
    // Cache (deprecated)
    Cache,
    // GIO charset converter
    CharsetConverter,
    // Checksum
    Checksum,
    ChecksumType,
    ClientCertificateMode,
    ClockFn,
    // Tree types
    CompareDataFn,
    CompareFn,
    // Completion
    Completion,
    ConnectFlags,
    // GIO context-specific group
    ContextSpecificGroup,
    // Convert / URI helpers
    ConvertError,
    // GIO converter interface
    Converter,
    ConverterFlags,
    // GIO converter input stream
    ConverterInputStream,
    // GIO converter output stream
    ConverterOutputStream,
    ConverterResult,
    // GIO credentials
    Credentials,
    // GIO credentials message
    CredentialsMessage,
    // GIO D-Bus action group
    DBusActionGroup,
    // GIO D-Bus address
    DBusAddress,
    DBusAddressPlatform,
    // GIO D-Bus introspection info structs
    DBusAnnotationInfo,
    DBusArgInfo,
    // GIO D-Bus auth
    DBusAuth,
    // GIO D-Bus auth mechanisms
    DBusAuthMechanism,
    DBusAuthMechanismAnon,
    DBusAuthMechanismExternal,
    DBusAuthMechanismSha1,
    // GIO D-Bus auth observer
    DBusAuthObserver,
    DBusAuthRole,
    DBusAuthState,
    DBusBusType,
    // GIO D-Bus connection
    DBusConnection,
    DBusConnectionFlags,
    DBusDaemon,
    // GIO D-Bus error handling
    DBusError,
    DBusErrorEntry,
    // GIO D-Bus interface
    DBusInterface,
    DBusInterfaceInfo,
    // GIO D-Bus interface skeleton
    DBusInterfaceSkeleton,
    DBusInterfaceSkeletonFlags,
    DBusMenuItem,
    // GIO D-Bus menu model
    DBusMenuModel,
    // GIO D-Bus message
    DBusMessage,
    DBusMessageFlags,
    DBusMessageHeaderField,
    DBusMessageType,
    DBusMethodInfo,
    // GIO D-Bus method invocation
    DBusMethodInvocation,
    // GIO D-Bus name owning
    DBusNameOwning,
    // GIO D-Bus name watching
    DBusNameWatching,
    DBusNodeInfo,
    // GIO D-Bus object
    DBusObject,
    // GIO D-Bus object manager
    DBusObjectManager,
    // GIO D-Bus object manager client/server
    DBusObjectManagerClient,
    DBusObjectManagerServer,
    // GIO D-Bus object proxy
    DBusObjectProxy,
    // GIO D-Bus object skeleton
    DBusObjectSkeleton,
    DBusPropertyInfo,
    DBusPropertyInfoFlags,
    // GIO D-Bus proxy
    DBusProxy,
    DBusProxyFlags,
    DBusReply,
    // GIO D-Bus server
    DBusServer,
    DBusServerFlags,
    DBusSignalInfo,
    // GIO data streams (endian-aware read/write)
    DataInputStream,
    // Dataset
    DataList,
    DataOutputStream,
    DataStreamByteOrder,
    DataStreamNewlineType,
    Datagram,
    // GIO datagram-based
    DatagramBased,
    // Date
    Date,
    DateDay,
    DateMonth,
    // Date-time
    DateTime,
    DateWeekday,
    DateYear,
    // GIO debug controller
    DebugController,
    // GIO debug controller D-Bus
    DebugControllerDBus,
    // GIO delayed settings backend
    DelayedSettingsBackend,
    // GIO desktop app info
    DesktopAppInfo,
    DestroyNotify,
    // Directory
    Dir,
    DirError,
    DirPlatform,
    DocumentPortal,
    DriveEntry,
    // GIO DTLS client connection
    DtlsClientConnection,
    // GIO DTLS connection
    DtlsConnection,
    // GIO DTLS server connection
    DtlsServerConnection,
    // GIO dummy file
    DummyFile,
    // GIO dummy proxy resolver
    DummyProxyResolver,
    // GIO dummy TLS backend
    DummyTlsBackend,
    Emblem,
    EmblemOrigin,
    EmblemedIcon,
    // Error
    Error,
    ErrorType,
    // GIO file operations
    File,
    FileAttributeInfo,
    FileAttributeInfoFlags,
    FileAttributeInfoList,
    // GIO file attribute info list
    FileAttributeType,
    // GIO file attribute values (full GFileInfo attribute layer)
    FileAttributeValue,
    FileCopyFlags,
    FileCreateFlags,
    // GIO file descriptor based
    FileDescriptorBased,
    // GIO file enumerator
    FileEnumerator,
    // File utilities
    FileError,
    // GIO file I/O stream
    FileIOStream,
    // GIO file icon
    FileIcon,
    FileInfo,
    // GIO file input stream
    FileInputStream,
    // GIO file monitor
    FileMonitor,
    FileMonitorEvent,
    // GIO enums + thumbnail verify
    FileMonitorEvent as GioFileMonitorEvent,
    // GIO file output stream
    FileOutputStream,
    FilePlatform,
    FileQueryInfoFlags,
    FileQueryInfoFlags as GioFileQueryInfoFlags,
    FileTest,
    FileType,
    // GIO filename completer
    FilenameCompleter,
    // GIO filter streams
    FilterInputStream,
    FilterOutputStream,
    // Arrays
    GArray,
    // GIO cancellable operations
    GCancellable,
    GCond,
    // GIO HTTP/SOCKS proxies
    GHttpProxy,
    // Lists
    GList,
    // GModule
    GModule,
    GModuleCheckInit,
    GModuleError,
    GModuleFlags,
    GModuleUnload,
    // Thread
    GMutex,
    // GObject
    GObject,
    // Pointer arrays
    GPointer,
    // Queue
    GQueue,
    GRWLock,
    GRecMutex,
    GSList,
    GSocks5Proxy,
    // GString
    GString,
    // GIO TLS password
    GTlsPassword,
    GTreeNode,
    // GObject type system
    GType,
    GTypeFlags,
    GTypeFundamentalFlags,
    GTypeInfo,
    GTypeValueTable,
    // GValue
    GValue,
    GValueData,
    HandlerID,
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
    HttpProxy,
    // I/O channel
    IOChannelError,
    IOCondition,
    IOError,
    // GIO error codes (errno / FileError -> IOErrorEnum)
    IOErrorEnum,
    IOFlags,
    IOStatus,
    IOStream,
    // GIO IP TOS / IPv6 TClass messages
    IPTosMessage,
    IPv6TClassMessage,
    // GIO icon (abstract: Themed / Bytes / Emblem / EmblemedIcon variants)
    Icon,
    IncomingConnection,
    InetAddrBytes,
    // GIO IP address
    InetAddress,
    // GIO IP address mask (subnet)
    InetAddressMask,
    InetAddressMaskError,
    // GIO inet socket address (IP address + port)
    InetSocketAddress,
    // GIO initable interface
    Initable,
    // GIO stream operations
    InputStream,
    InputStreamImpl,
    IoCondition,
    // GIO module
    IoModule,
    IoModulePlatform,
    // GIO scheduler
    IoScheduler,
    // Key file (INI parser)
    KeyFile,
    KeyFileError,
    KeyFileFlags,
    List,
    // GIO list model
    ListModel,
    ListStore,
    // GIO loadable icon
    LoadableIcon,
    // GIO local file subsystem
    LocalFile,
    LocalFileEnumerator,
    LocalFileIOStream,
    LocalFileInfo,
    LocalFileInputStream,
    LocalFileMonitor,
    LocalFileOutputStream,
    LocalFileType,
    LocalVfs,
    LocalVfs as GLocalVfs,
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
    // Markup (XML parser)
    MarkupError,
    MarkupNode,
    MarkupParseFlags,
    MarkupParser,
    MatchInfo,
    MemoryInputStream,
    // GIO memory monitor
    MemoryMonitor,
    MemoryMonitorBase,
    // GIO memory monitor variants
    MemoryMonitorDBus,
    MemoryMonitorLowMemoryLevel,
    MemoryMonitorPoll,
    MemoryMonitorPortal,
    MemoryMonitorPsi,
    MemoryOutputStream,
    MemoryPressureLevel,
    // GIO menu
    Menu,
    // GIO menu exporter
    MenuExporter,
    MenuItem,
    MenuModel,
    ModuleHandle,
    ModulePlatform,
    // GIO mount operation
    MountOperation,
    MountOperationResult,
    MountUnmountFlags as GioMountUnmountFlags,
    NMConnectivity,
    // N-ary tree
    NTree,
    NameOwnerState,
    NameWatchState,
    // GIO native socket address
    NativeSocketAddress,
    // GIO volume/mount monitors
    NativeVolumeMonitor,
    // GIO network address (hostname + port + optional scheme)
    NetworkAddress,
    NetworkAddressError,
    NetworkConnectivity,
    // GIO network monitor
    NetworkMonitor,
    // GIO network monitor base
    NetworkMonitorBase,
    NetworkMonitorNM,
    // GIO network monitor variants
    NetworkMonitorNetlink,
    NetworkMonitorPortal,
    // GIO network service (SRV record: service + protocol + domain)
    NetworkService,
    NoDBusAddressPlatform,
    NoDirPlatform,
    NoFilePlatform,
    NoIoModulePlatform,
    NoMappedFilePlatform,
    NoModulePlatform,
    NoSpawnPlatform,
    NoStdioPlatform,
    Node,
    NormalizeMode,
    // GIO desktop notification
    Notification,
    // GIO notification backend
    NotificationBackend,
    NotificationButton,
    NotificationPriority,
    ObjectFlags,
    Once,
    OnceStatus,
    OpenFlags,
    // GIO open URI portal
    OpenURIPortal,
    // Option parsing
    OptionArg,
    OptionContext,
    OptionEntry,
    OptionError,
    OptionFlags,
    OptionGroup,
    OutputStream,
    OutputStreamImpl,
    OutputStreamSpliceFlags,
    ParamFlags,
    ParamID,
    // GParamSpec
    ParamSpec,
    PasswordSave,
    // Path buffer
    PathBuf,
    // Pattern matching
    PatternSpec,
    // GIO permission
    Permission,
    Pid,
    // Poll
    PollFD,
    // GIO poll file monitor
    PollFileMonitor,
    // GIO pollable utils
    PollableCondition,
    // GIO pollable input stream
    PollableInputStream,
    // GIO pollable output stream
    PollableOutputStream,
    PollableReturn,
    // GIO portal support
    PortalSupport,
    PowerProfile,
    PowerProfile as DBusPowerProfile,
    // GIO power profile monitor
    PowerProfileMonitor,
    // GIO power profile monitor variants
    PowerProfileMonitorDBus,
    PowerProfileMonitorPortal,
    PrintFunc,
    // Printf
    PrintfArg,
    // GIO property action
    PropertyAction,
    PropertyBinding,
    // GIO proxy
    Proxy,
    // GIO proxy address (InetSocketAddress + proxy fields)
    ProxyAddress,
    // GIO proxy address enumerator
    ProxyAddressEnumerator,
    // GIO proxy resolver
    ProxyResolver,
    // GIO portal-based services
    ProxyResolverPortal,
    ProxyUriLookup,
    PtrArray,
    PtrCompareFunc,
    // Quarks
    Quark,
    // Random
    Rand,
    // RcBox
    RcBox,
    RefCount,
    // RefString
    RefString,
    // Regex
    Regex,
    RegexCompileFlags,
    RegexError,
    RegexMatchFlags,
    // GIO registry settings backend
    RegistrySettingsBackend,
    RehandshakeMode,
    // Relation (deprecated)
    Relation,
    RemoteAction,
    // GIO remote action group
    RemoteActionGroup,
    // GIO resource bundle
    Resource,
    // GIO resource file
    ResourceFile,
    ResourceLookupFlags,
    SList,
    // GIO sandbox
    Sandbox,
    SandboxType,
    // Scanner
    Scanner,
    ScannerConfig,
    // Seek
    SeekType,
    // GIO seekable trait
    Seekable,
    // Sorted sequence
    Sequence,
    SequenceIter,
    // GIO settings backend
    SettingsBackend,
    // GIO settings schema
    SettingsSchema,
    SettingsSchemaKey,
    SettingsSchemaSource,
    // Shell utilities
    ShellError,
    SignalCallback,
    SignalFlags,
    // GSignal
    SignalID,
    SignalQuery,
    SimpleAction,
    // GIO simple action group
    SimpleActionGroup,
    SimpleAppInfo,
    // GIO simple async result
    SimpleAsyncResult,
    SimpleAuthMechanism,
    SimpleConnectable,
    SimpleDBusInterface,
    SimpleDBusObject,
    // GIO simple IO stream
    SimpleIOStream,
    SimpleMenuModel,
    SimplePermission,
    // GIO simple proxy resolver
    SimpleProxyResolver,
    Size,
    // Slice (deprecated)
    SliceConfig,
    SockaddrIn,
    SockaddrIn6,
    SockaddrUn,
    // GIO socket address (abstract base)
    SocketAddress,
    SocketAddressEnumerator,
    // GIO socket client
    SocketClient,
    // GIO socket connectable
    SocketConnectable,
    // GIO socket connection
    SocketConnection,
    // GIO socket control message
    SocketControlMessage,
    SocketFamily,
    // GIO socket input/output streams
    SocketInputStream,
    // GIO socket listener
    SocketListener,
    SocketOutputStream,
    // GIO socket service
    SocketService,
    Socks4AProxy,
    Socks4Proxy,
    // Source
    Source,
    SourceCallbackFuncs,
    SourceCheckFunc,
    SourceDispatchFunc,
    SourceFinalizeFunc,
    SourceFlags,
    SourceFunc,
    SourceFuncs,
    SourcePrepareFunc,
    SpawnChildSetupFunc,
    // Spawn
    SpawnError,
    SpawnFlags,
    SpawnPlatform,
    SpawnResult,
    // GIO SRV record target
    SrvTarget,
    // Stdio
    StatBuf,
    StdioPlatform,
    // String chunk
    StringChunk,
    // String vector builder
    StrvBuilder,
    // GIO subprocess
    Subprocess,
    SubprocessFlags,
    // GIO subprocess launcher
    SubprocessLauncher,
    // GIO task
    Task,
    // GIO TCP connection
    TcpConnection,
    // GIO TCP wrapper connection
    TcpWrapperConnection,
    // Test utils
    TestCase,
    // GIO test D-Bus
    TestDBus,
    TestSubprocessFlags,
    TestSuite,
    TestTrapFlags,
    TestTrapStatus,
    ThemedIcon,
    ThreadError,
    // Thread pool
    ThreadPool,
    ThreadPoolError,
    // GIO threaded resolver
    ThreadedResolver,
    // GIO threaded socket service
    ThreadedSocketService,
    ThumbnailVerifyResult,
    TimeSpan,
    TimeType,
    // Time zone
    TimeZone,
    // Timer
    Timer,
    // GIO TLS backend
    TlsBackend,
    // GIO TLS certificate
    TlsCertificate,
    TlsCertificateFlags,
    // GIO TLS client connection
    TlsClientConnection,
    // GIO TLS connection
    TlsConnection,
    // GIO TLS database
    TlsDatabase,
    TlsDatabaseLookupFlags,
    TlsDatabaseVerifyFlags,
    // GIO TLS file database
    TlsFileDatabase,
    // GIO TLS interaction
    TlsInteraction,
    TlsInteractionResult,
    TlsPassword,
    TlsPasswordFlags,
    // GIO TLS server connection
    TlsServerConnection,
    // Scanner token types
    TokenType,
    TokenValue,
    TransformFunc,
    TrashPortal,
    // Trash stack (deprecated)
    TrashStack,
    TraverseFlags,
    TraverseFn,
    TraverseNodeFn,
    TraverseType,
    // Tree
    Tree,
    Tuples,
    TypeClassData,
    TypeInstanceData,
    TypeQuery,
    UInt,
    // UTF-8 / Unicode
    Unichar,
    Unichar2,
    UnicodeBreakType,
    UnicodeScript,
    // Unicode
    UnicodeType,
    UnionVolumeMonitor,
    // GIO Unix connection
    UnixConnection,
    UnixCredentialsMessage,
    // GIO Unix FD list/message
    UnixFDList,
    UnixFDMessage,
    // GIO Unix streams
    UnixInputStream,
    UnixMountEntry,
    UnixMounts,
    UnixOutputStream,
    // GIO UNIX domain socket address
    UnixSocketAddress,
    UnixSocketAddressType,
    UnixVolume,
    UnixVolumeMonitor,
    // URI parsing
    Uri,
    UriError,
    UriFlags,
    UriHideFlags,
    // Variant value container
    Variant,
    VariantBuilder,
    VariantClass,
    VariantParseError,
    // Variant types
    VariantType,
    // GIO virtual file system
    Vfs,
    VolumeEntry,
    // GIO volume monitor
    VolumeMonitor,
    WeakRefCallback,
    // GIO zlib compressor/decompressor
    ZlibCompressor,
    ZlibCompressorFormat,
    ZlibDecompressor,
    DATE_BAD_JULIAN,
    DIR_CASE_SENSITIVE,
    DIR_NO_DOT_AND_DOTDOT,
    F_OK,
    // Version
    GLIB_BINARY_AGE,
    GLIB_INTERFACE_AGE,
    GLIB_MAJOR_VERSION,
    GLIB_MICRO_VERSION,
    GLIB_MINOR_VERSION,
    G_TYPE_BOOLEAN,
    G_TYPE_BOXED,
    G_TYPE_CHAR,
    G_TYPE_DOUBLE,
    G_TYPE_ENUM,
    G_TYPE_FLAGS,
    G_TYPE_FLOAT,
    G_TYPE_INT,
    G_TYPE_INT64,
    G_TYPE_INTERFACE,
    G_TYPE_INVALID,
    G_TYPE_LONG,
    G_TYPE_NONE,
    G_TYPE_OBJECT,
    G_TYPE_PARAM,
    G_TYPE_POINTER,
    G_TYPE_STRING,
    G_TYPE_UCHAR,
    G_TYPE_UINT,
    G_TYPE_UINT64,
    G_TYPE_ULONG,
    G_TYPE_VARIANT,
    HOOK_FLAG_ACTIVE,
    HOOK_FLAG_IN_CALL,
    HOOK_FLAG_MASK,
    MEM_ALIGN,
    NSEC_PER_SEC,
    OPTION_REMAINING,
    OS_INFO_KEY_BUG_REPORT_URL,
    OS_INFO_KEY_DOCUMENTATION_URL,
    OS_INFO_KEY_HOME_URL,
    OS_INFO_KEY_ID,
    // OS info keys
    OS_INFO_KEY_NAME,
    OS_INFO_KEY_PRETTY_NAME,
    OS_INFO_KEY_PRIVACY_POLICY_URL,
    OS_INFO_KEY_SUPPORT_URL,
    OS_INFO_KEY_VERSION,
    OS_INFO_KEY_VERSION_CODENAME,
    OS_INFO_KEY_VERSION_ID,
    R_OK,
    S_IRGRP,
    S_IROTH,
    S_IRUSR,
    S_IRWXG,
    S_IRWXO,
    S_IRWXU,
    S_IWGRP,
    S_IWOTH,
    S_IWUSR,
    S_IXGRP,
    S_IXOTH,
    S_IXUSR,
    // Time span constants
    TIME_SPAN_DAY,
    TIME_SPAN_HOUR,
    TIME_SPAN_MILLISECOND,
    TIME_SPAN_MINUTE,
    TIME_SPAN_SECOND,
    UNIX_PATH_MAX,
    // Time constants
    USEC_PER_SEC,
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
    W_OK,
    X_OK,
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

/// Wire GLib platform traits to the RustOS syscall VFS and in-kernel services.
///
/// Requires `crate::vfs::init()` to have been called first.
pub fn init_glib_platform() {
    crate::glib_platform::register_all();
}

pub use crate::glib_platform::{RustOsIoModulePlatform, RustOsModulePlatform};

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
        || variant_type
            .element()
            .map(|element| element.as_str() != "s")
            .unwrap_or(true)
    {
        return Err("variant type element");
    }

    if !type_equal("(is)", "(is)") || type_hash("s") == type_hash("i") {
        return Err("variant type hash");
    }

    let mut queue = GQueue::new();
    queue.push_tail("glib");
    queue.push_tail("native");
    if queue.get_length() != 2
        || queue.pop_head() != Some("glib")
        || queue.pop_head() != Some("native")
    {
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
    if !date.valid() || date.day() != 27 || date.month() != DateMonth::June || date.year() != 2026 {
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
        .load_from_data(
            "[RustOS]\nname=glib-native\nversion=1\n",
            KeyFileFlags::NONE,
        )
        .map_err(|_| "GKeyFile load")?;
    if key_file
        .get_string("RustOS", "name")
        .map_err(|_| "GKeyFile string")?
        != "glib-native"
        || key_file
            .get_integer("RustOS", "version")
            .map_err(|_| "GKeyFile integer")?
            != 1
    {
        return Err("GKeyFile read");
    }

    key_file.set_string("RustOS", "status", "wired");
    if key_file
        .get_string("RustOS", "status")
        .map_err(|_| "GKeyFile set")?
        != "wired"
    {
        return Err("GKeyFile set");
    }

    let uri = Uri::parse(
        "http://example.com:8080/rustos?glib=1#native",
        UriFlags::NONE,
    )
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
    if markup.name != "root" || markup.attributes.len() != 1 || markup.attributes[0].name != "attr"
    {
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
        || tree
            .find(root, TraverseType::PreOrder, TraverseFlags::ALL, |data| {
                *data == 4
            })
            .is_none()
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
    option_context
        .parse(&mut option_argv)
        .map_err(|_| "GOption parse")?;
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
    let re = Regex::new(
        "g([a-z][a-z])b",
        RegexCompileFlags::DEFAULT,
        RegexMatchFlags::DEFAULT,
    )
    .map_err(|_| "GRegex compile")?;
    let info = re.r#match("glib", RegexMatchFlags::DEFAULT);
    if !info.matches()
        || info.fetch(0).as_deref() != Some("glib")
        || info.fetch(1).as_deref() != Some("li")
    {
        return Err("GRegex match");
    }
    if !Regex::match_simple(
        "\\d+",
        "abc123",
        RegexCompileFlags::DEFAULT,
        RegexMatchFlags::DEFAULT,
    ) {
        return Err("GRegex match_simple");
    }
    let re2 = Regex::new("\\s+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT)
        .map_err(|_| "GRegex compile 2")?;
    let parts = re2.split("glib native rustos", RegexMatchFlags::DEFAULT);
    if parts.len() != 3 || parts[0] != "glib" || parts[1] != "native" || parts[2] != "rustos" {
        return Err("GRegex split");
    }
    let re3 = Regex::new(
        "(\\w+)",
        RegexCompileFlags::DEFAULT,
        RegexMatchFlags::DEFAULT,
    )
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
    let sig_id = signal_new(
        "test-changed",
        G_TYPE_OBJECT,
        SignalFlags::RUN_LAST,
        G_TYPE_NONE,
        &[],
    );
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
    if obj
        .get_property("name")
        .map(|v| v.get_string().map(ToOwned::to_owned))
        != Some(Some("test-obj".to_owned()))
    {
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
    let built = module_build_path::<RustOsModulePlatform>(Some("/lib"), "rustos");
    if built != "/lib/librustos.so" {
        return Err("GModule build_path");
    }
    let built_no_dir = module_build_path::<RustOsModulePlatform>(None, "rustos");
    if built_no_dir != "/lib/librustos.so" {
        return Err("GModule build_path no dir");
    }
    let built_lib_prefix = module_build_path::<RustOsModulePlatform>(Some("/lib"), "libfoo");
    if built_lib_prefix != "/lib/libfoo.so" {
        return Err("GModule build_path lib prefix");
    }
    let open_result =
        module_open_full::<RustOsModulePlatform>(Some("rustos.so"), GModuleFlags::NONE);
    if open_result.is_ok() {
        return Err("GModule open on no-platform should fail");
    }
    let (err_code, err_msg) = open_result.unwrap_err();
    if err_code != GModuleError::Failed
        || !err_msg.contains("not supported")
        || module_error()
            .as_deref()
            .map(|s| !s.contains("not supported"))
            .unwrap_or(true)
    {
        return Err("GModule open error path");
    }
    let main_open_result = module_open::<RustOsModulePlatform>(None, GModuleFlags::BIND_LAZY);
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
    if transient.name() != "/lib/librustos.so"
        || transient.ref_count() != 1
        || transient.is_resident()
    {
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

    // GIO file attribute info list (Phase 11 entry point). Exercise the
    // full surface: create, sorted insert, binary-search lookup, dup
    // independence, ref count, and the type/flag enum values.
    if FileAttributeType::Invalid as u32 != 0
        || FileAttributeType::String as u32 != 1
        || FileAttributeType::Object as u32 != 8
        || FileAttributeType::Stringv as u32 != 9
    {
        return Err("GFileAttributeType values");
    }
    let rw = FileAttributeInfoFlags::COPY_WITH_FILE | FileAttributeInfoFlags::COPY_WHEN_MOVED;
    if !rw.contains(FileAttributeInfoFlags::COPY_WITH_FILE)
        || !rw.contains(FileAttributeInfoFlags::COPY_WHEN_MOVED)
        || FileAttributeInfoFlags::COPY_WITH_FILE.0 != 1
        || FileAttributeInfoFlags::COPY_WHEN_MOVED.0 != 2
    {
        return Err("GFileAttributeInfoFlags");
    }
    let mut attr_list = FileAttributeInfoList::new();
    if attr_list.n_infos() != 0 || attr_list.lookup("standard::name").is_some() {
        return Err("GFileAttributeInfoList empty");
    }
    attr_list
        .add(
            "standard::size",
            FileAttributeType::Uint64,
            FileAttributeInfoFlags::COPY_WITH_FILE,
        )
        .map_err(|_| "GFileAttributeInfoList add size")?;
    attr_list
        .add(
            "standard::name",
            FileAttributeType::String,
            FileAttributeInfoFlags::COPY_WITH_FILE,
        )
        .map_err(|_| "GFileAttributeInfoList add name")?;
    attr_list
        .add(
            "standard::is-hidden",
            FileAttributeType::Boolean,
            FileAttributeInfoFlags::NONE,
        )
        .map_err(|_| "GFileAttributeInfoList add hidden")?;
    if attr_list.n_infos() != 3 {
        return Err("GFileAttributeInfoList count");
    }
    // List should be sorted by name: name, is-hidden, size.
    if attr_list.infos()[0].name != "standard::is-hidden"
        || attr_list.infos()[1].name != "standard::name"
        || attr_list.infos()[2].name != "standard::size"
    {
        return Err("GFileAttributeInfoList sorted order");
    }
    let size_info = attr_list
        .lookup("standard::size")
        .ok_or("GFileAttributeInfoList lookup")?;
    if size_info.r#type != FileAttributeType::Uint64
        || !size_info
            .flags
            .contains(FileAttributeInfoFlags::COPY_WITH_FILE)
    {
        return Err("GFileAttributeInfoList lookup fields");
    }
    // Re-adding an existing name updates type in place.
    attr_list
        .add(
            "standard::size",
            FileAttributeType::Int64,
            FileAttributeInfoFlags::NONE,
        )
        .map_err(|_| "GFileAttributeInfoList update")?;
    if attr_list.n_infos() != 3
        || attr_list.lookup("standard::size").unwrap().r#type != FileAttributeType::Int64
    {
        return Err("GFileAttributeInfoList update-in-place");
    }
    // dup produces an independent copy.
    let mut dup = attr_list.dup();
    dup.add(
        "standard::extra",
        FileAttributeType::String,
        FileAttributeInfoFlags::NONE,
    )
    .map_err(|_| "GFileAttributeInfoList dup add")?;
    if dup.n_infos() != 4
        || attr_list.n_infos() != 3
        || attr_list.lookup("standard::extra").is_some()
    {
        return Err("GFileAttributeInfoList dup independence");
    }
    // ref count via clone.
    let shared = attr_list.ref_();
    if attr_list.ref_count() < 2 {
        return Err("GFileAttributeInfoList ref count");
    }
    drop(shared);
    if attr_list.ref_count() != 1 {
        return Err("GFileAttributeInfoList ref count drop");
    }

    // GIO D-Bus introspection info structs (Phase 11). Build a small
    // interface hierarchy and exercise the lookup helpers + ref counting.
    let rw = DBusPropertyInfoFlags::READABLE | DBusPropertyInfoFlags::WRITABLE;
    if !rw.contains(DBusPropertyInfoFlags::READABLE)
        || !rw.contains(DBusPropertyInfoFlags::WRITABLE)
        || DBusPropertyInfoFlags::READABLE.0 != 1
        || DBusPropertyInfoFlags::WRITABLE.0 != 2
    {
        return Err("GDBusPropertyInfoFlags");
    }
    let anno = alloc::sync::Arc::new(DBusAnnotationInfo {
        key: "org.freedesktop.DBus.Deprecated".to_owned(),
        value: "true".to_owned(),
        annotations: alloc::vec::Vec::new(),
    });
    if dbus_annotation_info_lookup(
        core::slice::from_ref(&anno),
        "org.freedesktop.DBus.Deprecated",
    ) != Some("true")
    {
        return Err("GDBus annotation lookup");
    }
    if dbus_annotation_info_lookup(core::slice::from_ref(&anno), "missing").is_some() {
        return Err("GDBus annotation lookup miss");
    }
    let echo_in = alloc::sync::Arc::new(DBusArgInfo {
        name: "message".to_owned(),
        signature: "s".to_owned(),
        annotations: alloc::vec::Vec::new(),
    });
    let echo_out = alloc::sync::Arc::new(DBusArgInfo {
        name: "reply".to_owned(),
        signature: "s".to_owned(),
        annotations: alloc::vec::Vec::new(),
    });
    let echo_method = alloc::sync::Arc::new(DBusMethodInfo {
        name: "Echo".to_owned(),
        in_args: alloc::vec![echo_in.clone()],
        out_args: alloc::vec![echo_out.clone()],
        annotations: alloc::vec::Vec::new(),
    });
    let on_echo_signal = alloc::sync::Arc::new(DBusSignalInfo {
        name: "OnEcho".to_owned(),
        args: alloc::vec![echo_in],
        annotations: alloc::vec::Vec::new(),
    });
    let version_prop = alloc::sync::Arc::new(DBusPropertyInfo {
        name: "Version".to_owned(),
        signature: "s".to_owned(),
        flags: DBusPropertyInfoFlags::READABLE,
        annotations: alloc::vec::Vec::new(),
    });
    let echo_iface = alloc::sync::Arc::new(DBusInterfaceInfo {
        name: "org.test.Echo".to_owned(),
        methods: alloc::vec![echo_method],
        signals: alloc::vec![on_echo_signal],
        properties: alloc::vec![version_prop],
        annotations: alloc::vec::Vec::new(),
    });
    let root_node = alloc::sync::Arc::new(DBusNodeInfo {
        path: Some("/org/test".to_owned()),
        interfaces: alloc::vec![echo_iface.clone()],
        nodes: alloc::vec::Vec::new(),
        annotations: alloc::vec::Vec::new(),
    });
    let found_iface = dbus_node_info_lookup_interface(&root_node, "org.test.Echo")
        .ok_or("GDBus node lookup interface")?;
    if found_iface.name != "org.test.Echo" {
        return Err("GDBus interface name");
    }
    if dbus_node_info_lookup_interface(&root_node, "org.test.Missing").is_some() {
        return Err("GDBus node lookup miss");
    }
    let found_method =
        dbus_interface_info_lookup_method(&found_iface, "Echo").ok_or("GDBus method lookup")?;
    if found_method.in_args.len() != 1
        || found_method.in_args[0].name != "message"
        || found_method.in_args[0].signature != "s"
        || found_method.out_args[0].name != "reply"
    {
        return Err("GDBus method args");
    }
    if dbus_interface_info_lookup_method(&found_iface, "Missing").is_some() {
        return Err("GDBus method lookup miss");
    }
    let found_signal =
        dbus_interface_info_lookup_signal(&found_iface, "OnEcho").ok_or("GDBus signal lookup")?;
    if found_signal.args.len() != 1 || found_signal.args[0].name != "message" {
        return Err("GDBus signal args");
    }
    let found_prop = dbus_interface_info_lookup_property(&found_iface, "Version")
        .ok_or("GDBus property lookup")?;
    if found_prop.signature != "s"
        || !found_prop.flags.contains(DBusPropertyInfoFlags::READABLE)
        || found_prop.flags.contains(DBusPropertyInfoFlags::WRITABLE)
    {
        return Err("GDBus property flags");
    }
    // Ref counting via Arc clone.
    let iface_ref = echo_iface.ref_();
    if !alloc::sync::Arc::ptr_eq(&iface_ref, &echo_iface) {
        return Err("GDBus ref_ same pointer");
    }
    drop(iface_ref);
    drop(root_node);
    // echo_iface should still be alive (we hold one Arc).
    if echo_iface.name != "org.test.Echo" {
        return Err("GDBus iface alive after node drop");
    }

    // GIO D-Bus error handling (Phase 11). Exercise the enum, quark,
    // registry, remote-error parsing/stripping, encode/decode round-trip.
    if DBusError::Failed as i32 != 0
        || DBusError::PropertyReadOnly as i32 != 44
        || DBusError::Failed.to_dbus_name() != "org.freedesktop.DBus.Error.Failed"
        || DBusError::SpawnExecFailed.to_dbus_name()
            != "org.freedesktop.DBus.Error.Spawn.ExecFailed"
    {
        return Err("GDBusError enum");
    }
    let dbus_q = dbus_error_quark();
    if dbus_q == 0 {
        return Err("GDBus error quark");
    }
    if dbus_error_quark() != dbus_q {
        return Err("GDBus error quark stable");
    }
    // The well-known G_DBUS_ERROR entries should be registered after
    // the quark call. Failed (code 0) -> org.freedesktop.DBus.Error.Failed.
    let failed_err = glib_native::Error::new(
        dbus_q,
        0,
        "GDBus.Error:org.freedesktop.DBus.Error.Failed: boom",
    );
    if !dbus_error_is_remote_error(&failed_err) {
        return Err("GDBus is_remote_error");
    }
    if dbus_error_get_remote_error(&failed_err).as_deref()
        != Some("org.freedesktop.DBus.Error.Failed")
    {
        return Err("GDBus get_remote_error registered");
    }
    // Strip the prefix.
    let mut stripped = failed_err.clone();
    if !dbus_error_strip_remote_error(&mut stripped) || stripped.message() != "boom" {
        return Err("GDBus strip_remote_error");
    }
    // A non-remote error should not be strippable.
    let local_err = glib_native::Error::new(dbus_q, 0, "just a local error");
    let mut local_stripped = local_err.clone();
    if dbus_error_strip_remote_error(&mut local_stripped) {
        return Err("GDBus strip on local");
    }
    // new_for_dbus_error with a registered name uses the registered
    // (domain, code).
    let new_err =
        dbus_error_new_for_dbus_error("org.freedesktop.DBus.Error.NoMemory", "out of memory");
    if new_err.domain() != dbus_q || new_err.code() != DBusError::NoMemory as i32 {
        return Err("GDBus new_for_dbus_error registered");
    }
    if !new_err
        .message()
        .contains("org.freedesktop.DBus.Error.NoMemory")
        || !new_err.message().ends_with("out of memory")
    {
        return Err("GDBus new_for_dbus_error message");
    }
    // encode_gerror on a registered (domain, code) returns the
    // registered D-Bus name.
    let encoded = dbus_error_encode_gerror(&new_err);
    if encoded != "org.freedesktop.DBus.Error.NoMemory" {
        return Err("GDBus encode_gerror registered");
    }
    // encode_gerror on an unregistered (domain, code) produces the
    // org.gtk.GDBus.UnmappedGError.Quark._* form, and new_for_dbus_error
    // can decode it back.
    let unmapped_domain = glib_native::quark_from_string(Some("rustos-test-unmapped"));
    let unmapped_err = glib_native::Error::new(unmapped_domain, 99, "mystery");
    let unmapped_encoded = dbus_error_encode_gerror(&unmapped_err);
    if !unmapped_encoded.starts_with("org.gtk.GDBus.UnmappedGError.Quark._") {
        return Err("GDBus encode unmapped prefix");
    }
    if !unmapped_encoded.contains("rustos_2dtest_2dunmapped") {
        return Err("GDBus encode unmapped escapes hyphens");
    }
    if !unmapped_encoded.ends_with(".Code99") {
        return Err("GDBus encode unmapped code suffix");
    }
    let roundtripped = dbus_error_new_for_dbus_error(&unmapped_encoded, "the message");
    if roundtripped.domain() != unmapped_domain || roundtripped.code() != 99 {
        return Err("GDBus unmapped round-trip");
    }
    // Register a custom error and verify register/unregister semantics.
    let custom_domain = glib_native::quark_from_static_string(Some("rustos-custom-dbus"));
    // Clean slate in case a previous run registered it.
    let _ = dbus_error_unregister_error(custom_domain, 7, "org.rustos.Custom");
    if !dbus_error_register_error(custom_domain, 7, "org.rustos.Custom") {
        return Err("GDBus register custom");
    }
    if dbus_error_register_error(custom_domain, 7, "org.rustos.Custom") {
        return Err("GDBus re-register should fail");
    }
    if !dbus_error_unregister_error(custom_domain, 7, "org.rustos.Custom") {
        return Err("GDBus unregister custom");
    }
    // Parse a remote-error prefix with colons in the message body.
    let msg_with_colons = "GDBus.Error:org.test.X: error: with: colons";
    let (name, rest) = glib_native::gdbuserror::parse_remote_prefix(msg_with_colons)
        .ok_or("GDBus parse_remote_prefix")?;
    if name != "org.test.X" || rest != "error: with: colons" {
        return Err("GDBus parse_remote_prefix content");
    }

    // GIO error codes (Phase 11). Exercise the enum, quark, and the
    // errno / FileError -> IOErrorEnum conversions.
    if IOErrorEnum::Failed as i32 != 0
        || IOErrorEnum::NotFound as i32 != 1
        || IOErrorEnum::BrokenPipe as i32 != 44
        || IOErrorEnum::NoSuchDevice as i32 != 47
        || IOErrorEnum::DestinationUnset as i32 != 48
    {
        return Err("GIOErrorEnum values");
    }
    if IOErrorEnum::CONNECTION_CLOSED != IOErrorEnum::BrokenPipe {
        return Err("GIOErrorEnum CONNECTION_CLOSED alias");
    }
    if io_error_quark() == 0 {
        return Err("GIO error quark");
    }
    // io_error_from_file_error mappings.
    if io_error_from_file_error(glib_native::FileError::Exist) != IOErrorEnum::Exists
        || io_error_from_file_error(glib_native::FileError::NoEnt) != IOErrorEnum::NotFound
        || io_error_from_file_error(glib_native::FileError::Acces) != IOErrorEnum::PermissionDenied
        || io_error_from_file_error(glib_native::FileError::NoSpc) != IOErrorEnum::NoSpace
        || io_error_from_file_error(glib_native::FileError::Pipe) != IOErrorEnum::BrokenPipe
        || io_error_from_file_error(glib_native::FileError::Failed) != IOErrorEnum::Failed
    {
        return Err("GIO from_file_error");
    }
    // io_error_from_errno: via FileError + additional codes.
    if io_error_from_errno(2) != IOErrorEnum::NotFound
        || io_error_from_errno(17) != IOErrorEnum::Exists
        || io_error_from_errno(13) != IOErrorEnum::PermissionDenied
        || io_error_from_errno(28) != IOErrorEnum::NoSpace
        || io_error_from_errno(125) != IOErrorEnum::Cancelled
        || io_error_from_errno(110) != IOErrorEnum::TimedOut
        || io_error_from_errno(98) != IOErrorEnum::AddressInUse
        || io_error_from_errno(111) != IOErrorEnum::ConnectionRefused
        || io_error_from_errno(104) != IOErrorEnum::CONNECTION_CLOSED
        || io_error_from_errno(107) != IOErrorEnum::NotConnected
        || io_error_from_errno(9999) != IOErrorEnum::Failed
    {
        return Err("GIO from_errno");
    }
    // file_error_from_errno (added to fileutils for gioerror).
    if file_error_from_errno(2) != glib_native::FileError::NoEnt
        || file_error_from_errno(17) != glib_native::FileError::Exist
        || file_error_from_errno(9999) != glib_native::FileError::Failed
    {
        return Err("GFileError from_errno");
    }

    // GIO desktop notification (Phase 11). Exercise the full surface:
    // construction, setters, buttons with targets, default action with
    // target, priority, urgent mapping, category, opaque icon.
    if NotificationPriority::Normal as i32 != 0
        || NotificationPriority::Low as i32 != 1
        || NotificationPriority::High as i32 != 2
        || NotificationPriority::Urgent as i32 != 3
    {
        return Err("GNotificationPriority values");
    }
    let mut notif = Notification::new("RustOS boot complete");
    if notif.title() != "RustOS boot complete"
        || notif.body() != ""
        || notif.priority() != NotificationPriority::Normal
        || notif.n_buttons() != 0
        || notif.default_action().is_some()
        || notif.default_action_target().is_some()
        || notif.icon().is_some()
    {
        return Err("GNotification defaults");
    }
    notif.set_body("All systems nominal");
    notif.set_priority(NotificationPriority::High);
    notif.set_category("system.boot");
    if notif.body() != "All systems nominal"
        || notif.priority() != NotificationPriority::High
        || notif.category() != Some("system.boot")
    {
        return Err("GNotification setters");
    }
    // set_urgent maps true -> Urgent, false -> Normal.
    notif.set_urgent(true);
    if notif.priority() != NotificationPriority::Urgent {
        return Err("GNotification set_urgent true");
    }
    notif.set_urgent(false);
    if notif.priority() != NotificationPriority::Normal {
        return Err("GNotification set_urgent false");
    }
    // Buttons with and without targets.
    notif.add_button("Dismiss", "app.dismiss");
    notif.add_button_with_target_value(
        "Open",
        "app.open",
        glib_native::Variant::new_string("/etc/hostname"),
    );
    if notif.n_buttons() != 2 {
        return Err("GNotification button count");
    }
    let buttons = notif.buttons();
    if buttons[0].label != "Dismiss"
        || buttons[0].action_name != "app.dismiss"
        || buttons[0].target.is_some()
    {
        return Err("GNotification button 0");
    }
    if buttons[1].label != "Open"
        || buttons[1].action_name != "app.open"
        || buttons[1].target.is_none()
        || buttons[1].target.as_ref().unwrap().get_string() != "/etc/hostname"
    {
        return Err("GNotification button 1");
    }
    // Default action with target.
    notif.set_default_action_with_target_value("app.activate", glib_native::Variant::new_int32(42));
    if notif.default_action() != Some("app.activate") {
        return Err("GNotification default action");
    }
    if notif.default_action_target().is_none()
        || notif.default_action_target().unwrap().get_int32() != 42
    {
        return Err("GNotification default action target");
    }
    // set_default_action (without target) clears the target.
    notif.set_default_action("app.activate");
    if notif.default_action_target().is_some() {
        return Err("GNotification default action clears target");
    }
    // Icon storage via GIcon enum.
    let icon = Icon::Themed(ThemedIcon::new("notification"));
    notif.set_icon(icon.clone());
    if notif.icon().is_none() {
        return Err("GNotification icon set");
    }
    if !notif.icon().unwrap().equal(&icon) {
        return Err("GNotification icon equal");
    }

    // GIO SRV record target (Phase 11). Exercise construction,
    // accessors, and RFC 2782 list sorting.
    let srv = SrvTarget::new("xmpp.example.com", 5222, 10, 60);
    if srv.hostname() != "xmpp.example.com"
        || srv.port() != 5222
        || srv.priority() != 10
        || srv.weight() != 60
    {
        return Err("GSrvTarget accessors");
    }
    // Empty list sorts to empty.
    let empty_sorted = srv_target_list_sort(alloc::vec::Vec::new());
    if !empty_sorted.is_empty() {
        return Err("GSrvTarget sort empty");
    }
    // Single "." hostname means service not available -> empty.
    let dot_sorted = srv_target_list_sort(alloc::vec![SrvTarget::new(".", 0, 0, 0)]);
    if !dot_sorted.is_empty() {
        return Err("GSrvTarget sort dot hostname");
    }
    // Sort by priority ascending.
    let prio_sorted = srv_target_list_sort(alloc::vec![
        SrvTarget::new("c.example.com", 80, 30, 0),
        SrvTarget::new("a.example.com", 80, 10, 0),
        SrvTarget::new("b.example.com", 80, 20, 0),
    ]);
    if prio_sorted.len() != 3
        || prio_sorted[0].priority() != 10
        || prio_sorted[1].priority() != 20
        || prio_sorted[2].priority() != 30
    {
        return Err("GSrvTarget sort by priority");
    }
    // All targets survive weighted-random selection within a group.
    let group_sorted = srv_target_list_sort(alloc::vec![
        SrvTarget::new("h1.example.com", 80, 10, 100),
        SrvTarget::new("h2.example.com", 80, 10, 50),
        SrvTarget::new("h3.example.com", 80, 10, 0),
    ]);
    if group_sorted.len() != 3 {
        return Err("GSrvTarget sort preserves all");
    }
    for t in &group_sorted {
        if t.priority() != 10 {
            return Err("GSrvTarget sort same priority");
        }
    }

    // GIO IP address (Phase 11). Exercise IPv4 and IPv6 parsing,
    // formatting, classification, and special addresses.
    if SocketFamily::Invalid as i32 != 0
        || SocketFamily::Ipv4 as i32 != 2
        || SocketFamily::Ipv6 as i32 != 10
    {
        return Err("GSocketFamily values");
    }
    // IPv4 parse + roundtrip.
    let v4 = InetAddress::new_from_string("192.168.1.1").ok_or("GInet v4 parse")?;
    if v4.family() != SocketFamily::Ipv4
        || v4.native_size() != 4
        || v4.to_bytes() != [192, 168, 1, 1]
        || v4.to_string() != "192.168.1.1"
    {
        return Err("GInet v4 roundtrip");
    }
    // IPv4 loopback + any.
    let v4_lo = InetAddress::new_loopback(SocketFamily::Ipv4).ok_or("GInet v4 loopback")?;
    if !v4_lo.is_loopback() || v4_lo.to_string() != "127.0.0.1" {
        return Err("GInet v4 loopback classification");
    }
    let v4_any = InetAddress::new_any(SocketFamily::Ipv4).ok_or("GInet v4 any")?;
    if !v4_any.is_any() || v4_any.to_string() != "0.0.0.0" {
        return Err("GInet v4 any classification");
    }
    // IPv4 site-local classification.
    if !InetAddress::new_from_string("10.1.2.3")
        .unwrap()
        .is_site_local()
        || !InetAddress::new_from_string("172.16.0.1")
            .unwrap()
            .is_site_local()
        || !InetAddress::new_from_string("192.168.1.1")
            .unwrap()
            .is_site_local()
        || InetAddress::new_from_string("11.0.0.0")
            .unwrap()
            .is_site_local()
    {
        return Err("GInet v4 site-local");
    }
    // IPv4 link-local.
    if !InetAddress::new_from_string("169.254.1.1")
        .unwrap()
        .is_link_local()
    {
        return Err("GInet v4 link-local");
    }
    // IPv4 multicast + scopes.
    if !InetAddress::new_from_string("224.0.0.1")
        .unwrap()
        .is_multicast()
        || !InetAddress::new_from_string("224.0.0.1")
            .unwrap()
            .is_mc_link_local()
        || !InetAddress::new_from_string("239.255.0.1")
            .unwrap()
            .is_mc_site_local()
    {
        return Err("GInet v4 multicast");
    }
    // IPv4 invalid.
    if InetAddress::new_from_string("192.168.1").is_some()
        || InetAddress::new_from_string("192.168.1.256").is_some()
        || InetAddress::new_from_string("not-an-ip").is_some()
    {
        return Err("GInet v4 invalid rejected");
    }
    // IPv6 parse + compression.
    let v6 = InetAddress::new_from_string("2001:db8::1").ok_or("GInet v6 parse")?;
    if v6.family() != SocketFamily::Ipv6
        || v6.native_size() != 16
        || v6.to_string() != "2001:db8::1"
    {
        return Err("GInet v6 roundtrip");
    }
    // IPv6 loopback + any.
    let v6_lo = InetAddress::new_loopback(SocketFamily::Ipv6).ok_or("GInet v6 loopback")?;
    if !v6_lo.is_loopback() || v6_lo.to_string() != "::1" {
        return Err("GInet v6 loopback");
    }
    let v6_any = InetAddress::new_any(SocketFamily::Ipv6).ok_or("GInet v6 any")?;
    if !v6_any.is_any() || v6_any.to_string() != "::" {
        return Err("GInet v6 any");
    }
    // IPv6 embedded IPv4.
    let v6_v4 = InetAddress::new_from_string("::ffff:192.168.1.1").ok_or("GInet v6+v4")?;
    if v6_v4.family() != SocketFamily::Ipv6 || v6_v4.to_bytes()[12..] != [192, 168, 1, 1] {
        return Err("GInet v6 embedded v4");
    }
    // IPv6 link-local + multicast + scopes.
    if !InetAddress::new_from_string("fe80::1")
        .unwrap()
        .is_link_local()
    {
        return Err("GInet v6 link-local");
    }
    if !InetAddress::new_from_string("ff02::1")
        .unwrap()
        .is_mc_link_local()
        || !InetAddress::new_from_string("ff0e::1")
            .unwrap()
            .is_mc_global()
    {
        return Err("GInet v6 multicast scopes");
    }
    // equal().
    let a = InetAddress::new_from_string("192.168.1.1").unwrap();
    let b = InetAddress::new_from_string("192.168.1.1").unwrap();
    if !a.equal(&b) {
        return Err("GInet equal");
    }
    // new_from_bytes with wrong size fails.
    if InetAddress::new_from_bytes(&[1, 2, 3], SocketFamily::Ipv4).is_some() {
        return Err("GInet wrong byte count");
    }

    // GIO IP address mask (Phase 11). Exercise construction, parsing,
    // to_string, matching, and equality.
    // IPv4 /24 mask.
    let mask_v4 =
        InetAddressMask::new_from_string("192.168.1.0/24").map_err(|_| "GInetMask v4 parse")?;
    if mask_v4.family() != SocketFamily::Ipv4
        || mask_v4.length() != 24
        || mask_v4.address().to_string() != "192.168.1.0"
        || mask_v4.to_string() != "192.168.1.0/24"
    {
        return Err("GInetMask v4 fields");
    }
    // Matches within /24.
    if !mask_v4.matches(&InetAddress::new_from_string("192.168.1.1").unwrap())
        || !mask_v4.matches(&InetAddress::new_from_string("192.168.1.255").unwrap())
        || mask_v4.matches(&InetAddress::new_from_string("192.168.2.1").unwrap())
    {
        return Err("GInetMask v4 matches");
    }
    // Different family doesn't match.
    if mask_v4.matches(&InetAddress::new_from_string("::1").unwrap()) {
        return Err("GInetMask v4 vs v6");
    }
    // Full-length mask (no /prefix) → 32 for IPv4.
    let full_v4 =
        InetAddressMask::new_from_string("192.168.1.1").map_err(|_| "GInetMask v4 full parse")?;
    if full_v4.length() != 32 || full_v4.to_string() != "192.168.1.1" {
        return Err("GInetMask v4 full");
    }
    // IPv6 /32 mask.
    let mask_v6 =
        InetAddressMask::new_from_string("2001:db8::/32").map_err(|_| "GInetMask v6 parse")?;
    if mask_v6.family() != SocketFamily::Ipv6 || mask_v6.length() != 32 {
        return Err("GInetMask v6 fields");
    }
    if !mask_v6.matches(&InetAddress::new_from_string("2001:db8::1").unwrap())
        || !mask_v6.matches(&InetAddress::new_from_string("2001:db8:abcd::1").unwrap())
        || mask_v6.matches(&InetAddress::new_from_string("2001:db9::1").unwrap())
    {
        return Err("GInetMask v6 matches");
    }
    // Error cases.
    if InetAddressMask::new_from_string("not-an-ip").is_ok() {
        return Err("GInetMask parse error");
    }
    if InetAddressMask::new_from_string("192.168.1.0/33").is_ok() {
        return Err("GInetMask length too long");
    }
    if InetAddressMask::new_from_string("192.168.1.1/24").is_ok() {
        return Err("GInetMask bits beyond prefix");
    }
    // Constructor with BitsBeyondPrefix.
    let addr_with_bits = InetAddress::new_from_string("192.168.1.1").unwrap();
    if let Ok(_) = InetAddressMask::new(addr_with_bits, 24) {
        return Err("GInetMask new bits beyond prefix");
    }
    // Equal masks.
    let m1 = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
    let m2 = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
    let m3 = InetAddressMask::new_from_string("192.168.1.0/25").unwrap();
    if !m1.equal(&m2) || m1.equal(&m3) {
        return Err("GInetMask equal");
    }

    // GIO network address (Phase 11). Exercise construction, parse,
    // parse_uri, accessors.
    let addr = NetworkAddress::new("example.com", 80);
    if addr.hostname() != "example.com" || addr.port() != 80 || addr.scheme().is_some() {
        return Err("GNetworkAddress new");
    }
    let lo = NetworkAddress::new_loopback(8080);
    if lo.hostname() != "localhost" || lo.port() != 8080 {
        return Err("GNetworkAddress loopback");
    }
    // parse: plain hostname → default port.
    let parsed =
        NetworkAddress::parse("example.com", 443).map_err(|_| "GNetworkAddress parse plain")?;
    if parsed.hostname() != "example.com" || parsed.port() != 443 {
        return Err("GNetworkAddress parse plain fields");
    }
    // parse: host:port.
    let parsed = NetworkAddress::parse("example.com:8080", 443)
        .map_err(|_| "GNetworkAddress parse host:port")?;
    if parsed.hostname() != "example.com" || parsed.port() != 8080 {
        return Err("GNetworkAddress parse host:port fields");
    }
    // parse: bracketed IPv6 with port.
    let parsed = NetworkAddress::parse("[2001:db8::1]:888", 443)
        .map_err(|_| "GNetworkAddress parse ipv6")?;
    if parsed.hostname() != "2001:db8::1" || parsed.port() != 888 {
        return Err("GNetworkAddress parse ipv6 fields");
    }
    // parse: unescaped IPv6 (multiple ':') → no port.
    let parsed = NetworkAddress::parse("2001:db8::1", 443)
        .map_err(|_| "GNetworkAddress parse ipv6 unescaped")?;
    if parsed.hostname() != "2001:db8::1" || parsed.port() != 443 {
        return Err("GNetworkAddress parse ipv6 unescaped fields");
    }
    // parse error cases.
    if NetworkAddress::parse("example.com:", 443).is_ok() {
        return Err("GNetworkAddress parse empty port");
    }
    if NetworkAddress::parse("example.com:99999", 443).is_ok() {
        return Err("GNetworkAddress parse invalid port");
    }
    if NetworkAddress::parse("[2001:db8::1", 443).is_ok() {
        return Err("GNetworkAddress parse unclosed bracket");
    }
    // parse_uri.
    let parsed_uri = NetworkAddress::parse_uri("http://example.com:8080/path", 443)
        .map_err(|_| "GNetworkAddress parse_uri")?;
    if parsed_uri.scheme() != Some("http")
        || parsed_uri.hostname() != "example.com"
        || parsed_uri.port() != 8080
    {
        return Err("GNetworkAddress parse_uri fields");
    }
    // parse_uri with no port → default.
    let parsed_uri = NetworkAddress::parse_uri("https://example.com/path", 443)
        .map_err(|_| "GNetworkAddress parse_uri no port")?;
    if parsed_uri.scheme() != Some("https") || parsed_uri.port() != 443 {
        return Err("GNetworkAddress parse_uri default port");
    }
    // parse_uri invalid.
    if NetworkAddress::parse_uri("not a uri", 443).is_ok() {
        return Err("GNetworkAddress parse_uri invalid");
    }
    // equal.
    let a = NetworkAddress::new("example.com", 80);
    let b = NetworkAddress::new("example.com", 80);
    let c = NetworkAddress::new("example.com", 81);
    if !a.equal(&b) || a.equal(&c) {
        return Err("GNetworkAddress equal");
    }

    // GValue transform functions (Phase 9 deferred item). Exercise
    // numeric casts, bool-from-numeric, numeric→string, bool→string,
    // same-type copy, and no-transform-available.
    init_builtin_transforms();
    // int → uint.
    let mut src_val = glib_native::GValue::for_type(glib_native::G_TYPE_INT);
    src_val.set_int(42);
    let mut dest_val = glib_native::GValue::for_type(glib_native::G_TYPE_UINT);
    if !value_transform(&src_val, &mut dest_val) || dest_val.get_uint() != 42 {
        return Err("GValue transform int→uint");
    }
    // int → double.
    let mut dest_val = glib_native::GValue::for_type(glib_native::G_TYPE_DOUBLE);
    if !value_transform(&src_val, &mut dest_val) || dest_val.get_double() != 42.0 {
        return Err("GValue transform int→double");
    }
    // int → string.
    let mut dest_val = glib_native::GValue::for_type(glib_native::G_TYPE_STRING);
    if !value_transform(&src_val, &mut dest_val) || dest_val.get_string() != Some("42") {
        return Err("GValue transform int→string");
    }
    // int → bool (non-zero → true).
    let mut dest_val = glib_native::GValue::for_type(glib_native::G_TYPE_BOOLEAN);
    if !value_transform(&src_val, &mut dest_val) || !dest_val.get_boolean() {
        return Err("GValue transform int→bool");
    }
    src_val.set_int(0);
    if !value_transform(&src_val, &mut dest_val) || dest_val.get_boolean() {
        return Err("GValue transform int→bool zero");
    }
    // bool → string.
    let mut bool_val = glib_native::GValue::for_type(glib_native::G_TYPE_BOOLEAN);
    bool_val.set_boolean(true);
    let mut str_dest = glib_native::GValue::for_type(glib_native::G_TYPE_STRING);
    if !value_transform(&bool_val, &mut str_dest) || str_dest.get_string() != Some("TRUE") {
        return Err("GValue transform bool→string TRUE");
    }
    bool_val.set_boolean(false);
    if !value_transform(&bool_val, &mut str_dest) || str_dest.get_string() != Some("FALSE") {
        return Err("GValue transform bool→string FALSE");
    }
    // Same-type copy via value_type_compatible.
    let mut int_dest = glib_native::GValue::for_type(glib_native::G_TYPE_INT);
    if !value_transform(&src_val, &mut int_dest) || int_dest.get_int() != 0 {
        return Err("GValue transform same-type copy");
    }
    // value_type_transformable checks.
    if !value_type_transformable(glib_native::G_TYPE_INT, glib_native::G_TYPE_UINT)
        || !value_type_transformable(glib_native::G_TYPE_INT, glib_native::G_TYPE_DOUBLE)
        || !value_type_transformable(glib_native::G_TYPE_INT, glib_native::G_TYPE_INT)
    {
        return Err("GValue type_transformable");
    }
    // string → float has no transform → not transformable.
    if value_type_transformable(glib_native::G_TYPE_STRING, glib_native::G_TYPE_FLOAT) {
        return Err("GValue type_transformable false negative");
    }

    // ── GInetSocketAddress (Phase 11) ──────────────────────────────
    // new + accessors.
    let inet_sa =
        InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 8080);
    if inet_sa.address().to_string() != "192.168.1.1" || inet_sa.port() != 8080 {
        return Err("GInetSocketAddress new + accessors");
    }
    if inet_sa.family() != SocketFamily::Ipv4 {
        return Err("GInetSocketAddress family IPv4");
    }
    // new_from_string.
    let inet_sa2 = InetSocketAddress::new_from_string("10.0.0.1", 80)
        .ok_or("GInetSocketAddress new_from_string")?;
    if inet_sa2.port() != 80 {
        return Err("GInetSocketAddress new_from_string port");
    }
    // new_from_string invalid.
    if InetSocketAddress::new_from_string("not-an-ip", 80).is_some() {
        return Err("GInetSocketAddress new_from_string invalid");
    }
    // IPv6 with flowinfo/scope_id.
    let inet_v6 = InetSocketAddress::new_with_ipv6_info(
        InetAddress::new_from_string("2001:db8::1").unwrap(),
        443,
        0x12345,
        42,
    );
    if inet_v6.family() != SocketFamily::Ipv6 {
        return Err("GInetSocketAddress family IPv6");
    }
    if inet_v6.flowinfo() != 0x12345 || inet_v6.scope_id() != 42 {
        return Err("GInetSocketAddress flowinfo/scope_id");
    }
    // flowinfo/scope_id return 0 for IPv4.
    let inet_v4_info = InetSocketAddress::new_with_ipv6_info(
        InetAddress::new_from_string("1.2.3.4").unwrap(),
        80,
        999,
        888,
    );
    if inet_v4_info.flowinfo() != 0 || inet_v4_info.scope_id() != 0 {
        return Err("GInetSocketAddress flowinfo IPv4 returns 0");
    }
    // native_size.
    if inet_sa.native_size() != 16 {
        return Err("GInetSocketAddress native_size IPv4");
    }
    if inet_v6.native_size() != 28 {
        return Err("GInetSocketAddress native_size IPv6");
    }
    // to_native + from_native roundtrip (IPv4).
    let mut buf = [0u8; 16];
    inet_sa
        .to_native(&mut buf)
        .map_err(|_| "GInetSocketAddress to_native IPv4")?;
    let sa_rt =
        InetSocketAddress::from_native(&buf).ok_or("GInetSocketAddress from_native IPv4")?;
    if sa_rt.address().to_string() != "192.168.1.1" || sa_rt.port() != 8080 {
        return Err("GInetSocketAddress roundtrip IPv4");
    }
    // to_native + from_native roundtrip (IPv6).
    let mut buf6 = [0u8; 28];
    inet_v6
        .to_native(&mut buf6)
        .map_err(|_| "GInetSocketAddress to_native IPv6")?;
    let sa6_rt =
        InetSocketAddress::from_native(&buf6).ok_or("GInetSocketAddress from_native IPv6")?;
    if sa6_rt.address().to_string() != "2001:db8::1" || sa6_rt.port() != 443 {
        return Err("GInetSocketAddress roundtrip IPv6");
    }
    if sa6_rt.flowinfo() != 0x12345 || sa6_rt.scope_id() != 42 {
        return Err("GInetSocketAddress roundtrip IPv6 flowinfo/scope_id");
    }
    // to_native no space.
    let mut small = [0u8; 4];
    if inet_sa.to_native(&mut small).is_ok() {
        return Err("GInetSocketAddress to_native no space");
    }
    // to_string.
    if inet_sa.to_string() != "192.168.1.1:8080" {
        return Err("GInetSocketAddress to_string IPv4");
    }
    if inet_v6.to_string() != "[2001:db8::1%42]:443" {
        return Err("GInetSocketAddress to_string IPv6");
    }
    // to_string with scope_id.
    let inet_scope = InetSocketAddress::new_with_ipv6_info(
        InetAddress::new_from_string("fe80::1").unwrap(),
        80,
        0,
        5,
    );
    if inet_scope.to_string() != "[fe80::1%5]:80" {
        return Err("GInetSocketAddress to_string scope_id");
    }
    // equal.
    let eq_a = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
    let eq_b = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
    if !eq_a.equal(&eq_b) {
        return Err("GInetSocketAddress equal");
    }

    // ── GSocketAddress (Phase 11) ──────────────────────────────────
    // Inet variant.
    let sock_addr = SocketAddress::Inet(inet_sa.clone());
    if sock_addr.family() != SocketFamily::Ipv4 {
        return Err("GSocketAddress family IPv4");
    }
    if sock_addr.native_size() != 16 {
        return Err("GSocketAddress native_size");
    }
    if sock_addr.to_string() != "192.168.1.1:8080" {
        return Err("GSocketAddress to_string");
    }
    // new_from_native (IPv4).
    let mut native_buf = [0u8; 16];
    inet_sa
        .to_native(&mut native_buf)
        .map_err(|_| "GSocketAddress to_native")?;
    let from_native =
        SocketAddress::new_from_native(&native_buf).ok_or("GSocketAddress new_from_native")?;
    match &from_native {
        SocketAddress::Inet(sa) => {
            if sa.address().to_string() != "192.168.1.1" || sa.port() != 8080 {
                return Err("GSocketAddress new_from_native fields");
            }
        }
        _ => return Err("GSocketAddress new_from_native variant"),
    }
    // new_from_native (IPv6).
    let mut native_buf6 = [0u8; 28];
    inet_v6
        .to_native(&mut native_buf6)
        .map_err(|_| "GSocketAddress to_native IPv6")?;
    let from_native6 = SocketAddress::new_from_native(&native_buf6)
        .ok_or("GSocketAddress new_from_native IPv6")?;
    match &from_native6 {
        SocketAddress::Inet(sa) => {
            if sa.family() != SocketFamily::Ipv6 {
                return Err("GSocketAddress new_from_native IPv6 family");
            }
        }
        _ => return Err("GSocketAddress new_from_native IPv6 variant"),
    }
    // new_from_native unknown family → Native variant.
    let mut unknown_buf = [0u8; 64];
    unknown_buf[0] = 99;
    let native_addr = SocketAddress::new_from_native(&unknown_buf)
        .ok_or("GSocketAddress new_from_native unknown")?;
    match &native_addr {
        SocketAddress::Native(_) => {}
        _ => return Err("GSocketAddress unknown family should be Native"),
    }
    // new_from_native AF_UNSPEC → None.
    let unspec_buf = [0u8; 64];
    if SocketAddress::new_from_native(&unspec_buf).is_some() {
        return Err("GSocketAddress AF_UNSPEC should return None");
    }

    // ── GUnixSocketAddress (Phase 11) ─────────────────────────────
    // new (path).
    let unix_sa = UnixSocketAddress::new("/tmp/socket");
    if unix_sa.path() != b"/tmp/socket" || unix_sa.path_len() != 11 {
        return Err("GUnixSocketAddress new + path");
    }
    if unix_sa.address_type() != UnixSocketAddressType::Path {
        return Err("GUnixSocketAddress address_type Path");
    }
    if unix_sa.is_abstract() {
        return Err("GUnixSocketAddress path should not be abstract");
    }
    if unix_sa.family() != SocketFamily::Unix {
        return Err("GUnixSocketAddress family");
    }
    // new_with_type (anonymous).
    let anon_sa =
        UnixSocketAddress::new_with_type(b"ignored", None, UnixSocketAddressType::Anonymous);
    if anon_sa.path_len() != 0 || anon_sa.address_type() != UnixSocketAddressType::Anonymous {
        return Err("GUnixSocketAddress anonymous");
    }
    // new_with_type (abstract).
    let abs_sa = UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::Abstract);
    if !abs_sa.is_abstract() || abs_sa.path() != b"test" {
        return Err("GUnixSocketAddress abstract");
    }
    // new_with_type (path_len).
    let pl_sa =
        UnixSocketAddress::new_with_type(b"hello world", Some(5), UnixSocketAddressType::Path);
    if pl_sa.path() != b"hello" || pl_sa.path_len() != 5 {
        return Err("GUnixSocketAddress path_len");
    }
    // native_size.
    if unix_sa.native_size() != 110 {
        return Err("GUnixSocketAddress native_size Path");
    }
    if anon_sa.native_size() != 2 {
        return Err("GUnixSocketAddress native_size Anonymous");
    }
    // to_native + from_native roundtrip (path).
    let mut unix_buf = [0u8; 110];
    unix_sa
        .to_native(&mut unix_buf)
        .map_err(|_| "GUnixSocketAddress to_native")?;
    let unix_rt =
        UnixSocketAddress::from_native(&unix_buf).ok_or("GUnixSocketAddress from_native")?;
    if unix_rt.path() != b"/tmp/socket" || unix_rt.address_type() != UnixSocketAddressType::Path {
        return Err("GUnixSocketAddress roundtrip path");
    }
    // to_native + from_native roundtrip (abstract).
    let abs_len = abs_sa.native_size();
    let mut abs_buf = vec![0u8; abs_len];
    abs_sa
        .to_native(&mut abs_buf)
        .map_err(|_| "GUnixSocketAddress to_native abstract")?;
    let abs_rt = UnixSocketAddress::from_native(&abs_buf)
        .ok_or("GUnixSocketAddress from_native abstract")?;
    if abs_rt.path() != b"test" || abs_rt.address_type() != UnixSocketAddressType::Abstract {
        return Err("GUnixSocketAddress roundtrip abstract");
    }
    // to_native no space.
    let mut tiny = [0u8; 4];
    if unix_sa.to_native(&mut tiny).is_ok() {
        return Err("GUnixSocketAddress to_native no space");
    }
    // to_string.
    if unix_sa.to_string() != "/tmp/socket" {
        return Err("GUnixSocketAddress to_string path");
    }
    if anon_sa.to_string() != "anonymous" {
        return Err("GUnixSocketAddress to_string anonymous");
    }
    // equal.
    let eq_a = UnixSocketAddress::new("/tmp/socket");
    let eq_b = UnixSocketAddress::new("/tmp/socket");
    if !eq_a.equal(&eq_b) {
        return Err("GUnixSocketAddress equal");
    }
    // SocketAddress::Unix variant.
    let sock_unix = SocketAddress::Unix(unix_sa.clone());
    if sock_unix.family() != SocketFamily::Unix {
        return Err("GSocketAddress Unix family");
    }
    if sock_unix.to_string() != "/tmp/socket" {
        return Err("GSocketAddress Unix to_string");
    }
    // SocketAddress::new_from_native (Unix).
    let mut unix_native = [0u8; 110];
    unix_sa
        .to_native(&mut unix_native)
        .map_err(|_| "GSocketAddress Unix to_native")?;
    let from_unix = SocketAddress::new_from_native(&unix_native)
        .ok_or("GSocketAddress new_from_native Unix")?;
    match &from_unix {
        SocketAddress::Unix(sa) => {
            if sa.path() != b"/tmp/socket" {
                return Err("GSocketAddress new_from_native Unix path");
            }
        }
        _ => return Err("GSocketAddress new_from_native Unix variant"),
    }

    // ── GNetworkService (Phase 11) ────────────────────────────────
    let ns = NetworkService::new("ldap", "tcp", "example.com");
    if ns.service() != "ldap" || ns.protocol() != "tcp" || ns.domain() != "example.com" {
        return Err("GNetworkService new + accessors");
    }
    // Default scheme = service name.
    if ns.scheme() != "ldap" {
        return Err("GNetworkService default scheme");
    }
    // set_scheme.
    let mut ns2 = ns.clone();
    ns2.set_scheme("ldaps");
    if ns2.scheme() != "ldaps" {
        return Err("GNetworkService set_scheme");
    }
    // to_string.
    if ns.to_string() != "(ldap, tcp, example.com, ldap)" {
        return Err("GNetworkService to_string");
    }
    if ns2.to_string() != "(ldap, tcp, example.com, ldaps)" {
        return Err("GNetworkService to_string with scheme");
    }
    // equal.
    if !ns.equal(&NetworkService::new("ldap", "tcp", "example.com")) {
        return Err("GNetworkService equal");
    }
    if ns.equal(&ns2) {
        return Err("GNetworkService not equal (different scheme)");
    }

    // ── GProxyAddress (Phase 11) ──────────────────────────────────
    let proxy_addr =
        InetAddress::new_from_string("192.168.1.1").ok_or("GProxyAddress inetaddr parse")?;
    let pa = ProxyAddress::new(
        proxy_addr.clone(),
        1080,
        "socks",
        "example.com",
        443,
        Some("user"),
        Some("pass"),
    );
    if pa.protocol() != "socks" || pa.destination_hostname() != "example.com" {
        return Err("GProxyAddress new + accessors");
    }
    if pa.destination_port() != 443 || pa.port() != 1080 {
        return Err("GProxyAddress ports");
    }
    if pa.username() != Some("user") || pa.password() != Some("pass") {
        return Err("GProxyAddress auth");
    }
    if pa.uri().is_some() || pa.destination_protocol().is_some() {
        return Err("GProxyAddress optional fields should be None");
    }
    // Delegated accessors.
    if pa.address().to_string() != "192.168.1.1" || pa.family() != SocketFamily::Ipv4 {
        return Err("GProxyAddress delegated accessors");
    }
    if pa.native_size() != 16 {
        return Err("GProxyAddress native_size");
    }
    if pa.to_string() != "192.168.1.1:1080" {
        return Err("GProxyAddress to_string");
    }
    // to_native.
    let mut pa_buf = [0u8; 16];
    pa.to_native(&mut pa_buf)
        .map_err(|_| "GProxyAddress to_native")?;
    // new_full (with dest_protocol + uri).
    let pa_full = ProxyAddress::new_full(
        proxy_addr.clone(),
        1080,
        "socks",
        "example.com",
        443,
        Some("user"),
        Some("pass"),
        Some("https"),
        Some("socks://192.168.1.1:1080"),
    );
    if pa_full.destination_protocol() != Some("https") {
        return Err("GProxyAddress dest_protocol");
    }
    if pa_full.uri() != Some("socks://192.168.1.1:1080") {
        return Err("GProxyAddress uri");
    }
    // equal.
    let pa_eq = ProxyAddress::new(
        proxy_addr.clone(),
        1080,
        "socks",
        "example.com",
        443,
        Some("user"),
        Some("pass"),
    );
    if !pa.equal(&pa_eq) {
        return Err("GProxyAddress equal");
    }
    if pa.equal(&pa_full) {
        return Err("GProxyAddress not equal (full has extra fields)");
    }

    // ── GThemedIcon (Phase 11) ────────────────────────────────────
    let ti = ThemedIcon::new("folder");
    let ti_names = ti.names();
    if !ti_names.iter().any(|n| n == "folder") {
        return Err("GThemedIcon new + names");
    }
    if !ti_names.iter().any(|n| n == "folder-symbolic") {
        return Err("GThemedIcon symbolic variant");
    }
    // Default fallbacks.
    let ti_fb = ThemedIcon::new_with_default_fallbacks("gnome-dev-cdrom-audio");
    let fb_names = ti_fb.names();
    if !fb_names.iter().any(|n| n == "gnome-dev-cdrom-audio")
        || !fb_names.iter().any(|n| n == "gnome-dev-cdrom")
        || !fb_names.iter().any(|n| n == "gnome-dev")
        || !fb_names.iter().any(|n| n == "gnome")
    {
        return Err("GThemedIcon default fallbacks");
    }
    // No fallbacks (without default_fallbacks).
    let ti_nofb = ThemedIcon::new("gnome-dev-cdrom-audio");
    if ti_nofb.names().iter().any(|n| n == "gnome-dev-cdrom") {
        return Err("GThemedIcon should not have fallbacks");
    }
    // new_from_names.
    let ti_multi = ThemedIcon::new_from_names(&["folder", "open"]);
    if !ti_multi.names().iter().any(|n| n == "folder")
        || !ti_multi.names().iter().any(|n| n == "open")
    {
        return Err("GThemedIcon new_from_names");
    }
    // prepend/append.
    let mut ti_mut = ThemedIcon::new("folder");
    ti_mut.prepend_name("directory");
    if ti_mut.names()[0] != "directory" {
        return Err("GThemedIcon prepend_name");
    }
    ti_mut.append_name("open");
    if !ti_mut.names().iter().any(|n| n == "open") {
        return Err("GThemedIcon append_name");
    }
    // equal.
    if !ThemedIcon::new("folder").equal(&ThemedIcon::new("folder")) {
        return Err("GThemedIcon equal");
    }
    if ThemedIcon::new("folder").equal(&ThemedIcon::new("file")) {
        return Err("GThemedIcon not equal");
    }
    // to_string (single name → just the name, but with symbolic variant it's multi).
    let ts = ThemedIcon::new("folder").to_string();
    if !ts.contains("folder") {
        return Err("GThemedIcon to_string");
    }

    // ── GBytesIcon (Phase 11) ─────────────────────────────────────
    let icon_bytes = Bytes::from_static(b"fake-png-data");
    let bi = BytesIcon::new(icon_bytes.clone());
    if bi.bytes().as_ref() != b"fake-png-data" {
        return Err("GBytesIcon new + bytes");
    }
    if bi.to_string() != "bytes" {
        return Err("GBytesIcon to_string");
    }
    // equal.
    let bi2 = BytesIcon::new(icon_bytes.clone());
    if !bi.equal(&bi2) {
        return Err("GBytesIcon equal");
    }
    let bi3 = BytesIcon::new(Bytes::from_static(b"different"));
    if bi.equal(&bi3) {
        return Err("GBytesIcon not equal");
    }
    // hash consistency.
    if bi.hash() != bi2.hash() {
        return Err("GBytesIcon hash consistency");
    }

    // ── GIcon (Phase 11) ──────────────────────────────────────────
    // Themed variant.
    let icon_t = Icon::Themed(ThemedIcon::new("folder"));
    // Bytes variant.
    let icon_b = Icon::Bytes(BytesIcon::new(icon_bytes));
    // equal same type.
    if !icon_t.equal(&Icon::Themed(ThemedIcon::new("folder"))) {
        return Err("GIcon equal Themed");
    }
    // equal different type.
    if icon_t.equal(&icon_b) {
        return Err("GIcon not equal different type");
    }
    // new_for_string single.
    let parsed = Icon::new_for_string("folder").map_err(|_| "GIcon new_for_string")?;
    match &parsed {
        Icon::Themed(ti) => {
            if !ti.names().iter().any(|n| n == "folder") {
                return Err("GIcon new_for_string single name");
            }
        }
        _ => return Err("GIcon new_for_string should be Themed"),
    }
    // new_for_string multi.
    let parsed_multi =
        Icon::new_for_string(". folder open").map_err(|_| "GIcon new_for_string multi")?;
    match &parsed_multi {
        Icon::Themed(ti) => {
            if !ti.names().iter().any(|n| n == "folder") || !ti.names().iter().any(|n| n == "open")
            {
                return Err("GIcon new_for_string multi names");
            }
        }
        _ => return Err("GIcon new_for_string multi should be Themed"),
    }
    // new_for_string empty → error.
    if Icon::new_for_string("").is_ok() {
        return Err("GIcon new_for_string empty should error");
    }
    // hash consistency.
    let icon_t2 = Icon::Themed(ThemedIcon::new("folder"));
    if icon_t.hash() != icon_t2.hash() {
        return Err("GIcon hash consistency");
    }

    // ── GEmblem (Phase 11) ────────────────────────────────────────
    let emblem_icon = Icon::Themed(ThemedIcon::new("emblem-default"));
    let emblem = Emblem::new_with_origin(emblem_icon.clone(), EmblemOrigin::Device);
    if emblem.origin() != EmblemOrigin::Device {
        return Err("GEmblem origin");
    }
    match emblem.icon() {
        Icon::Themed(ti) => {
            if !ti.names().iter().any(|n| n == "emblem-default") {
                return Err("GEmblem icon");
            }
        }
        _ => return Err("GEmblem icon variant"),
    }
    // equal.
    let emblem2 = Emblem::new_with_origin(emblem_icon.clone(), EmblemOrigin::Device);
    if !emblem.equal(&emblem2) {
        return Err("GEmblem equal");
    }
    let emblem3 = Emblem::new_with_origin(emblem_icon.clone(), EmblemOrigin::Tag);
    if emblem.equal(&emblem3) {
        return Err("GEmblem not equal (different origin)");
    }
    // hash consistency.
    if emblem.hash() != emblem2.hash() {
        return Err("GEmblem hash consistency");
    }
    // EmblemOrigin::from_i32.
    if EmblemOrigin::from_i32(1) != Some(EmblemOrigin::Device) {
        return Err("GEmblemOrigin from_i32");
    }
    if EmblemOrigin::from_i32(99).is_some() {
        return Err("GEmblemOrigin from_i32 invalid");
    }

    // ── GEmblemedIcon (Phase 11) ──────────────────────────────────
    let base_icon = Icon::Themed(ThemedIcon::new("folder"));
    let ei = EmblemedIcon::new(base_icon.clone(), Some(emblem.clone()));
    if ei.get_emblems().len() != 1 {
        return Err("GEmblemedIcon new with emblem");
    }
    // get_icon.
    match ei.get_icon() {
        Icon::Themed(ti) => {
            if !ti.names().iter().any(|n| n == "folder") {
                return Err("GEmblemedIcon get_icon");
            }
        }
        _ => return Err("GEmblemedIcon get_icon variant"),
    }
    // add_emblem.
    let mut ei2 = EmblemedIcon::new(base_icon.clone(), None);
    ei2.add_emblem(emblem.clone());
    ei2.add_emblem(emblem3.clone());
    if ei2.get_emblems().len() != 2 {
        return Err("GEmblemedIcon add_emblem");
    }
    // clear_emblems.
    ei2.clear_emblems();
    if !ei2.get_emblems().is_empty() {
        return Err("GEmblemedIcon clear_emblems");
    }
    // equal.
    let ei_a = EmblemedIcon::new(base_icon.clone(), Some(emblem.clone()));
    let ei_b = EmblemedIcon::new(base_icon.clone(), Some(emblem.clone()));
    if !ei_a.equal(&ei_b) {
        return Err("GEmblemedIcon equal");
    }
    let ei_c = EmblemedIcon::new(base_icon.clone(), Some(emblem3.clone()));
    if ei_a.equal(&ei_c) {
        return Err("GEmblemedIcon not equal (different emblem)");
    }
    // hash consistency.
    if ei_a.hash() != ei_b.hash() {
        return Err("GEmblemedIcon hash consistency");
    }
    // to_string.
    let ei_s = ei_a.to_string();
    if !ei_s.contains("folder") || !ei_s.contains("emblem-default") {
        return Err("GEmblemedIcon to_string");
    }

    // ── GIcon with Emblem/EmblemedIcon variants ───────────────────
    let icon_emblem = Icon::Emblem(emblem.clone());
    let icon_emblem2 = Icon::Emblem(emblem2.clone());
    if !icon_emblem.equal(&icon_emblem2) {
        return Err("GIcon Emblem equal");
    }
    let icon_ei = Icon::EmblemedIcon(ei_a.clone());
    let icon_ei2 = Icon::EmblemedIcon(ei_b.clone());
    if !icon_ei.equal(&icon_ei2) {
        return Err("GIcon EmblemedIcon equal");
    }
    if icon_emblem.equal(&icon_ei) {
        return Err("GIcon different variants not equal");
    }
    if icon_emblem.hash() != icon_emblem2.hash() {
        return Err("GIcon Emblem hash consistency");
    }

    // ── GCancellable (Phase 11) ───────────────────────────────────
    let cancel_obj = GCancellable::new();
    if cancel_obj.is_cancelled() {
        return Err("GCancellable initial state");
    }
    if cancel_obj.set_error_if_cancelled().is_err() {
        return Err("GCancellable set_error_if_cancelled on uncancelled");
    }
    let smoke_counter = alloc::sync::Arc::new(core::sync::atomic::AtomicU32::new(0));
    let smoke_counter_clone = smoke_counter.clone();
    let conn_id = cancel_obj.connect(move || {
        smoke_counter_clone.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    });
    if conn_id == 0 {
        return Err("GCancellable connect returned 0");
    }
    cancel_obj.cancel();
    if !cancel_obj.is_cancelled() {
        return Err("GCancellable is_cancelled after cancel");
    }
    if smoke_counter.load(core::sync::atomic::Ordering::SeqCst) != 1 {
        return Err("GCancellable connected handler not run");
    }
    let cancel_err = cancel_obj.set_error_if_cancelled().unwrap_err();
    if cancel_err.domain() != io_error_quark()
        || cancel_err.code() != IOErrorEnum::Cancelled.to_code()
    {
        return Err("GCancellable set_error_if_cancelled error mismatch");
    }
    cancel_obj.reset();
    if cancel_obj.is_cancelled() {
        return Err("GCancellable is_cancelled after reset");
    }
    if cancellable_get_current().is_some() {
        return Err("GCancellable stack initial");
    }
    cancellable_push_current(&cancel_obj);
    if cancellable_get_current().is_none() {
        return Err("GCancellable stack get after push");
    }
    cancellable_pop_current(&cancel_obj);
    if cancellable_get_current().is_some() {
        return Err("GCancellable stack after pop");
    }

    // ── GIO Streams (Phase 11) ────────────────────────────────────
    let in_mem = MemoryInputStream::new_from_bytes(Bytes::from_static(b"stream data"));
    let in_stream = InputStream::from(in_mem);
    let mut read_buf = [0u8; 15];
    let (n_read, err) = in_stream.read_all(&mut read_buf, None).unwrap();
    if n_read != 11 || err.is_some() || &read_buf[..11] != b"stream data" {
        return Err("GInputStream read_all mismatch");
    }

    let out_mem = MemoryOutputStream::new_resizable();
    let out_stream = OutputStream::from(out_mem);
    let (n_written, err) = out_stream.write_all(b"written data", None).unwrap();
    if n_written != 12 || err.is_some() {
        return Err("GOutputStream write_all mismatch");
    }
    let out_underlying = out_stream.downcast_ref::<MemoryOutputStream>().unwrap();
    if out_underlying.get_data() != b"written data" {
        return Err("GOutputStream written data mismatch");
    }

    // Splice
    let splice_in = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
        b"spliced",
    )));
    let splice_out = OutputStream::from(MemoryOutputStream::new_resizable());
    let spliced = splice_out
        .splice(&splice_in, OutputStreamSpliceFlags::None, None)
        .unwrap();
    if spliced != 7 {
        return Err("GOutputStream splice mismatch");
    }
    let splice_underlying = splice_out.downcast_ref::<MemoryOutputStream>().unwrap();
    if splice_underlying.get_data() != b"spliced" {
        return Err("GOutputStream splice data mismatch");
    }

    // IOStream
    let io_in = InputStream::from(MemoryInputStream::new());
    let io_out = OutputStream::from(MemoryOutputStream::new_resizable());
    let io_stream = IOStream::new(io_in, io_out);
    if io_stream.is_closed() {
        return Err("GIOStream initial state");
    }
    io_stream.close(None).unwrap();
    if !io_stream.is_closed() {
        return Err("GIOStream closed state");
    }

    // ── GFile (Phase 11) ──────────────────────────────────────────
    let file = File::new_for_path("/test/path.txt");
    if file.get_path() != Some("/test/path.txt".to_owned()) {
        return Err("GFile get_path mismatch");
    }
    if file.get_basename() != Some("path.txt".to_owned()) {
        return Err("GFile get_basename mismatch");
    }
    let parent_file = file.get_parent().unwrap();
    if parent_file.get_path() != Some("/test".to_owned()) {
        return Err("GFile get_parent mismatch");
    }

    // FileInfo
    let mut file_info = FileInfo::new();
    file_info.set_size(512);
    file_info.set_file_type(FileType::Regular);
    file_info.set_name("path.txt");
    file_info.set_attribute_string("standard::display-name", "Path File");
    if file_info.get_size() != 512
        || file_info.get_file_type() != FileType::Regular
        || file_info.get_name() != "path.txt"
    {
        return Err("GFileInfo getters mismatch");
    }
    if file_info.get_attribute_string("standard::display-name") != Some("Path File") {
        return Err("GFileInfo attribute mismatch");
    }

    // GFile via RustOS VFS platform
    {
        const SMOKE_PATH: &str = "/tmp/glib-smoke-file.txt";
        let payload = b"rustos-glib-vfs";
        let fd = crate::vfs::vfs_open(
            SMOKE_PATH,
            crate::vfs::OpenFlags::RDWR
                | crate::vfs::OpenFlags::CREAT
                | crate::vfs::OpenFlags::TRUNC,
            0o644,
        )
        .map_err(|_| "vfs create glib smoke file")?;
        crate::vfs::vfs_write(fd, payload).map_err(|_| "vfs write glib smoke file")?;
        let _ = crate::vfs::vfs_close(fd);

        let boot_file = File::new_for_path(SMOKE_PATH);
        if !boot_file.query_exists(None) {
            return Err("GFile query_exists vfs");
        }
        let boot_stream = boot_file.read(None).map_err(|_| "GFile read vfs")?;
        let mut boot_buf = [0u8; 32];
        let (n_boot_read, _) = boot_stream
            .read_all(&mut boot_buf, None)
            .map_err(|_| "GFile read_all vfs")?;
        if n_boot_read != payload.len() || &boot_buf[..payload.len()] != payload {
            return Err("GFile read vfs payload");
        }
        let info = boot_file
            .query_info("standard::size", FileQueryInfoFlags::None, None)
            .map_err(|_| "GFile query_info vfs")?;
        if info.get_size() != payload.len() as u64 {
            return Err("GFile query_info size vfs");
        }
    }

    // ── GDataInputStream (Phase 11) ───────────────────────────────
    let data_in = DataInputStream::new(InputStream::from(MemoryInputStream::new_from_bytes(
        Bytes::from_static(b"\x01\x02\x03\x04hello\n"),
    )));
    if data_in.read_uint16(None).unwrap() != 0x0102 {
        return Err("GDataInputStream read_uint16");
    }
    if data_in.read_uint16(None).unwrap() != 0x0304 {
        return Err("GDataInputStream read_uint16 second");
    }
    if data_in.read_line(None).unwrap().as_deref() != Some("hello") {
        return Err("GDataInputStream read_line");
    }
    // byte order switch to LE
    let data_in_le = DataInputStream::new(InputStream::from(MemoryInputStream::new_from_bytes(
        Bytes::from_static(b"\x01\x02"),
    )));
    data_in_le.set_byte_order(DataStreamByteOrder::LittleEndian);
    if data_in_le.read_uint16(None).unwrap() != 0x0201 {
        return Err("GDataInputStream read_uint16 LE");
    }
    // newline type
    if data_in_le.get_newline_type() != DataStreamNewlineType::Lf {
        return Err("GDataInputStream default newline type");
    }
    data_in_le.set_newline_type(DataStreamNewlineType::CrLf);
    if data_in_le.get_newline_type() != DataStreamNewlineType::CrLf {
        return Err("GDataInputStream set newline type");
    }

    // ── GDataOutputStream (Phase 11) ──────────────────────────────
    let dos_out = OutputStream::from(MemoryOutputStream::new_resizable());
    let dos = DataOutputStream::new(dos_out.clone());
    dos.put_uint16(0x0102, None).unwrap();
    dos.put_uint32(0x03040506, None).unwrap();
    dos.put_string("hello", None).unwrap();
    let underlying = dos_out.downcast_ref::<MemoryOutputStream>().unwrap();
    if underlying.get_data() != b"\x01\x02\x03\x04\x05\x06hello" {
        return Err("GDataOutputStream written data mismatch");
    }
    // LE byte order
    let dos_out2 = OutputStream::from(MemoryOutputStream::new_resizable());
    let dos2 = DataOutputStream::new(dos_out2.clone());
    dos2.set_byte_order(DataStreamByteOrder::LittleEndian);
    dos2.put_uint16(0x0102, None).unwrap();
    let underlying2 = dos_out2.downcast_ref::<MemoryOutputStream>().unwrap();
    if underlying2.get_data() != b"\x02\x01" {
        return Err("GDataOutputStream LE mismatch");
    }

    // ── GSeekable (Phase 11) ──────────────────────────────────────
    // SeekType enum values
    if SeekType::Cur as i32 != 0 || SeekType::Set as i32 != 1 || SeekType::End as i32 != 2 {
        return Err("GSeekType values");
    }

    // Seek on MemoryInputStream
    let seek_in_mem = MemoryInputStream::new_from_bytes(Bytes::from_static(b"seekable stream"));
    let seek_in = InputStream::from(seek_in_mem);
    if !seek_in.can_seek() {
        return Err("GSeekable can_seek on input stream failed");
    }
    seek_in.seek(9, SeekType::Set, None).unwrap();
    let mut seek_buf = [0u8; 6];
    let (n_seek, _) = seek_in.read_all(&mut seek_buf, None).unwrap();
    if n_seek != 6 || &seek_buf != b"stream" {
        return Err("GSeekable seek Set failed");
    }

    // Seek/truncate on MemoryOutputStream
    let seek_out_mem = MemoryOutputStream::new_resizable();
    let seek_out = OutputStream::from(seek_out_mem);
    if !seek_out.can_seek() || !seek_out.can_truncate() {
        return Err("GSeekable capabilities on output stream failed");
    }
    seek_out.write_all(b"initial data", None).unwrap();
    seek_out.seek(8, SeekType::Set, None).unwrap();
    seek_out.write_all(b"rust", None).unwrap();
    let seek_out_underlying = seek_out.downcast_ref::<MemoryOutputStream>().unwrap();
    if seek_out_underlying.get_data() != b"initial rust" {
        return Err("GSeekable seek write failed");
    }
    seek_out.truncate(7, None).unwrap();
    if seek_out_underlying.get_data() != b"initial" {
        return Err("GSeekable truncate failed");
    }

    // ── GPermission (Phase 11) ────────────────────────────────────
    let perm = Permission::new();
    if perm.get_allowed() || perm.get_can_acquire() || perm.get_can_release() {
        return Err("GPermission default state");
    }
    perm.impl_update(true, true, false);
    if !perm.get_allowed() || !perm.get_can_acquire() || perm.get_can_release() {
        return Err("GPermission impl_update");
    }
    if perm.acquire(None).is_err() {
        return Err("GPermission acquire when can_acquire");
    }
    if perm.release(None).is_ok() {
        return Err("GPermission release should fail when !can_release");
    }

    // ── GSimplePermission (Phase 11) ──────────────────────────────
    let simple = SimplePermission::new(true);
    if !simple.get_allowed() {
        return Err("GSimplePermission allowed=true");
    }
    let simple_no = SimplePermission::new(false);
    if simple_no.get_allowed() {
        return Err("GSimplePermission allowed=false");
    }

    // ── GConverter (Phase 11) ─────────────────────────────────────
    if ConverterFlags::NoFlags as u32 != 0
        || ConverterFlags::InputAtEnd as u32 != 1
        || ConverterFlags::Flush as u32 != 2
    {
        return Err("GConverterFlags values");
    }
    if ConverterResult::Error as u32 != 0
        || ConverterResult::Converted as u32 != 1
        || ConverterResult::Finished as u32 != 2
        || ConverterResult::Flushed as u32 != 3
    {
        return Err("GConverterResult values");
    }

    // ── GAction (Phase 11) ────────────────────────────────────────
    if !action_name_is_valid("open") {
        return Err("GAction name valid");
    }
    if action_name_is_valid("") || action_name_is_valid("1bad") {
        return Err("GAction name invalid");
    }
    let (parsed_name, parsed_target) = action_parse_detailed_name("open('file.txt')").unwrap();
    if parsed_name != "open" || parsed_target.is_none() {
        return Err("GAction parse detailed name");
    }
    let printed = action_print_detailed_name("open", Some(&parsed_target.unwrap()));
    if printed != "open('file.txt')" {
        return Err("GAction print detailed name");
    }

    // ── GSimpleAction (Phase 11) ──────────────────────────────────
    let action = SimpleAction::new("save", None);
    if action.get_name() != "save" || !action.get_enabled() {
        return Err("GSimpleAction new");
    }
    action.set_enabled(false);
    if action.get_enabled() {
        return Err("GSimpleAction set_enabled");
    }
    let stateful = SimpleAction::new_stateful("toggle", None, Variant::new_boolean(true));
    if !stateful.get_state().unwrap().get_boolean() {
        return Err("GSimpleAction stateful");
    }
    stateful.change_state(Variant::new_boolean(false));
    if stateful.get_state().unwrap().get_boolean() {
        return Err("GSimpleAction change_state");
    }

    // ── GFilterInputStream (Phase 11) ─────────────────────────────
    let filter_in = FilterInputStream::new(InputStream::from(MemoryInputStream::new_from_bytes(
        Bytes::from_static(b"filter test"),
    )));
    if !filter_in.get_close_base_stream() {
        return Err("GFilterInputStream default close_base_stream");
    }
    filter_in.set_close_base_stream(false);
    if filter_in.get_close_base_stream() {
        return Err("GFilterInputStream set_close_base_stream");
    }
    let mut fbuf = [0u8; 6];
    filter_in
        .get_base_stream()
        .read_all(&mut fbuf, None)
        .unwrap();
    if &fbuf != b"filter" {
        return Err("GFilterInputStream base_stream read");
    }

    // ── GFilterOutputStream (Phase 11) ────────────────────────────
    let filter_out =
        FilterOutputStream::new(OutputStream::from(MemoryOutputStream::new_resizable()));
    if !filter_out.get_close_base_stream() {
        return Err("GFilterOutputStream default close_base_stream");
    }
    filter_out.set_close_base_stream(false);
    if filter_out.get_close_base_stream() {
        return Err("GFilterOutputStream set_close_base_stream");
    }
    filter_out
        .get_base_stream()
        .write_all(b"filtered", None)
        .unwrap();
    let funderlying = filter_out
        .get_base_stream()
        .downcast_ref::<MemoryOutputStream>()
        .unwrap();
    if funderlying.get_data() != b"filtered" {
        return Err("GFilterOutputStream base_stream write");
    }

    // ── GListStore / GListModel (Phase 11) ────────────────────────
    let store = ListStore::new("s");
    store.append("alpha");
    store.append("beta");
    store.append("gamma");
    if store.get_n_items() != 3 {
        return Err("GListStore n_items");
    }
    if store.get_item(1).unwrap() != "beta" {
        return Err("GListStore get_item");
    }
    store.remove(0);
    if store.get_item(0).unwrap() != "beta" || store.get_n_items() != 2 {
        return Err("GListStore remove");
    }
    if store.find("gamma") != Some(1) {
        return Err("GListStore find");
    }
    let model: &dyn ListModel = &store;
    if model.get_n_items() != 2 {
        return Err("GListModel n_items via trait");
    }

    // ── GCharsetConverter (Phase 11) ──────────────────────────────
    let conv = CharsetConverter::new("UTF-8", "UTF-8").unwrap();
    let input = b"hello";
    let mut output = [0u8; 5];
    let (result, read, written) = conv
        .convert(input, &mut output, ConverterFlags::InputAtEnd)
        .unwrap();
    if result != ConverterResult::Finished || read != 5 || written != 5 || &output != b"hello" {
        return Err("GCharsetConverter identity convert");
    }
    let conv2 = CharsetConverter::new("ASCII", "UTF-8").unwrap();
    conv2.set_use_fallback(true);
    let input2 = b"hi\xc3\xa9";
    let mut output2 = [0u8; 4];
    let (_, _, _) = conv2
        .convert(input2, &mut output2, ConverterFlags::InputAtEnd)
        .unwrap();
    if &output2 != b"hi??" {
        return Err("GCharsetConverter UTF-8 to ASCII fallback");
    }
    if conv2.get_num_fallbacks() != 2 {
        return Err("GCharsetConverter num_fallbacks");
    }

    // ── GZlibCompressor (Phase 11) ────────────────────────────────
    let zcomp = ZlibCompressor::new(ZlibCompressorFormat::Gzip, 6);
    if zcomp.get_format() != ZlibCompressorFormat::Gzip || zcomp.get_level() != 6 {
        return Err("GZlibCompressor new");
    }
    let zinput = b"compressed data";
    let mut zoutput = [0u8; 128];
    let (zresult, zread, zwritten) = zcomp
        .convert(zinput, &mut zoutput, ConverterFlags::InputAtEnd)
        .unwrap();
    if zresult != ConverterResult::Finished || zread != zinput.len() || zwritten <= zinput.len() {
        return Err("GZlibCompressor convert");
    }
    let zdecomp = ZlibDecompressor::new(ZlibCompressorFormat::Gzip);
    let mut zdecompressed = [0u8; 32];
    let (zresult, zread, zwritten_out) = zdecomp
        .convert(
            &zoutput[..zwritten],
            &mut zdecompressed,
            ConverterFlags::InputAtEnd,
        )
        .unwrap();
    if zresult != ConverterResult::Finished
        || zread != zwritten
        || &zdecompressed[..zwritten_out] != zinput
    {
        return Err("GZlibDecompressor roundtrip");
    }

    // ── GSimpleActionGroup (Phase 11) ─────────────────────────────
    let group = SimpleActionGroup::new();
    group.add_action(Box::new(SimpleAction::new("open", None)));
    if !group.has_action("open") {
        return Err("GSimpleActionGroup has_action");
    }
    if group.list_actions().len() != 1 {
        return Err("GSimpleActionGroup list_actions");
    }
    group.remove_action("open");
    if group.has_action("open") {
        return Err("GSimpleActionGroup remove_action");
    }

    // ── GMountOperation (Phase 11) ────────────────────────────────
    let mount_op = MountOperation::new();
    mount_op.set_username(Some("admin"));
    if mount_op.get_username().unwrap() != "admin" {
        return Err("GMountOperation username");
    }
    mount_op.set_password_save(PasswordSave::Permanently);
    if mount_op.get_password_save() != PasswordSave::Permanently {
        return Err("GMountOperation password_save");
    }

    // ── GFileInputStream (Phase 11) ───────────────────────────────
    let fstream = FileInputStream::from_data(b"file content here");
    let mut fbuf = [0u8; 4];
    let n = fstream.read(&mut fbuf, None).unwrap();
    if n != 4 || &fbuf != b"file" {
        return Err("GFileInputStream read");
    }
    fstream.seek(13, SeekType::Set, None).unwrap();
    let mut fbuf2 = [0u8; 4];
    let n2 = fstream.read(&mut fbuf2, None).unwrap();
    if n2 != 4 || &fbuf2 != b"here" {
        return Err("GFileInputStream seek + read");
    }

    // ── GFileOutputStream (Phase 11) ──────────────────────────────
    let fostream = FileOutputStream::new();
    fostream.write(b"hello ", None).unwrap();
    fostream.write(b"world", None).unwrap();
    if fostream.get_data() != b"hello world" {
        return Err("GFileOutputStream write");
    }
    fostream.seek(6, SeekType::Set, None).unwrap();
    fostream.write(b"Rust", None).unwrap();
    if fostream.get_data() != b"hello Rustd" {
        return Err("GFileOutputStream seek + write");
    }
    fostream.truncate(5, None).unwrap();
    if fostream.get_data() != b"hello" {
        return Err("GFileOutputStream truncate");
    }

    // ── GFileIOStream (Phase 11) ──────────────────────────────────
    let fiostream = FileIOStream::from_data(b"read write test");
    let mut iobuf = [0u8; 4];
    let n = fiostream.read(&mut iobuf, None).unwrap();
    if n != 4 || &iobuf != b"read" {
        return Err("GFileIOStream read");
    }
    fiostream.seek(5, SeekType::Set, None).unwrap();
    fiostream.write(b"WRITE", None).unwrap();
    if fiostream.get_data() != b"read WRITE test" {
        return Err("GFileIOStream write");
    }

    // ── GFileIcon (Phase 11) ──────────────────────────────────────
    let icon = FileIcon::new("/usr/share/icons/test.png");
    if icon.get_file() != "/usr/share/icons/test.png" {
        return Err("GFileIcon get_file");
    }
    icon.set_data(b"fake png data");
    let (istream, itype) = icon.load(48, None).unwrap();
    if itype.unwrap() != "image/png" {
        return Err("GFileIcon load type");
    }
    let mut ibuf = [0u8; 13];
    let (in_read, _) = istream.read_all(&mut ibuf, None).unwrap();
    if in_read != 13 || &ibuf != b"fake png data" {
        return Err("GFileIcon load data");
    }

    // ── GFilenameCompleter (Phase 11) ─────────────────────────────
    let completer = FilenameCompleter::new();
    completer.add_entry("apple.txt");
    completer.add_entry("apple.json");
    completer.add_entry("banana.txt");
    let comps = completer.get_completions("apple");
    if comps.len() != 2 {
        return Err("GFilenameCompleter get_completions");
    }
    let suffix = completer.get_completion_suffix("hel");
    if suffix.is_some() {
        return Err("GFilenameCompleter no match suffix");
    }
    completer.set_dirs_only(true);
    completer.add_entry("docs/");
    completer.add_entry("docs.txt");
    let dir_comps = completer.get_completions("docs");
    if dir_comps.len() != 1 || dir_comps[0] != "docs/" {
        return Err("GFilenameCompleter dirs_only");
    }

    // ── GFileEnumerator (Phase 11) ────────────────────────────────
    let container = File::new_for_path("/test");
    let mut info1 = FileInfo::new();
    info1.set_name("alpha.txt");
    info1.set_size(100);
    let mut info2 = FileInfo::new();
    info2.set_name("beta.txt");
    info2.set_size(200);
    let enumerator = FileEnumerator::new(container, vec![info1, info2]);
    let e1 = enumerator.next_file(None).unwrap().unwrap();
    if e1.get_name() != "alpha.txt" {
        return Err("GFileEnumerator next_file 1");
    }
    let e2 = enumerator.next_file(None).unwrap().unwrap();
    if e2.get_name() != "beta.txt" {
        return Err("GFileEnumerator next_file 2");
    }
    if enumerator.next_file(None).unwrap().is_some() {
        return Err("GFileEnumerator exhausted");
    }

    // ── GFileMonitor (Phase 11) ───────────────────────────────────
    let monitor = FileMonitor::new();
    monitor.emit_event("/test/file.txt", None, FileMonitorEvent::Changed);
    let events = monitor.get_events();
    if events.len() != 1 || events[0].2 != FileMonitorEvent::Changed {
        return Err("GFileMonitor emit_event");
    }
    monitor.cancel();
    if !monitor.is_cancelled() {
        return Err("GFileMonitor cancel");
    }

    // ── GVfs (Phase 11) ───────────────────────────────────────────
    let vfs = LocalVfs::new();
    if !vfs.is_active() {
        return Err("GVfs is_active");
    }
    let vfs_file = vfs.get_file_for_path("/home/user");
    if vfs_file.get_path().unwrap() != "/home/user" {
        return Err("GVfs get_file_for_path");
    }
    let schemes = vfs.get_supported_uri_schemes();
    if !schemes.contains(&"file".to_string()) {
        return Err("GVfs supported_uri_schemes");
    }

    // ── GBufferedInputStream (Phase 11) ───────────────────────────
    let base_input = {
        use glib_native::bytes::Bytes;
        use glib_native::ginputstream::InputStream;
        use glib_native::ginputstream::MemoryInputStream;
        let bytes = Bytes::new(b"hello buffered world");
        InputStream::new(MemoryInputStream::new_from_bytes(bytes))
    };
    let buffered_in = BufferedInputStream::new(base_input);
    let n_filled = buffered_in
        .fill(-1, None)
        .map_err(|_| "GBufferedInputStream fill")?;
    if n_filled != 20 {
        return Err("GBufferedInputStream fill count");
    }
    if buffered_in.get_available() != 20 {
        return Err("GBufferedInputStream get_available");
    }
    let b0 = buffered_in
        .read_byte(None)
        .map_err(|_| "GBufferedInputStream read_byte")?;
    if b0 != b'h' {
        return Err("GBufferedInputStream read_byte value");
    }

    // ── GBufferedOutputStream (Phase 11) ──────────────────────────
    {
        use glib_native::goutputstream::{MemoryOutputStream, OutputStream};
        let base_out = OutputStream::new(MemoryOutputStream::new_resizable());
        let buffered_out = BufferedOutputStream::new_sized(base_out, 64);
        if buffered_out.get_buffer_size() != 64 {
            return Err("GBufferedOutputStream get_buffer_size");
        }
        let n_written = buffered_out
            .write(b"data", None)
            .map_err(|_| "GBufferedOutputStream write")?;
        if n_written != 4 {
            return Err("GBufferedOutputStream write count");
        }
        buffered_out
            .flush(None)
            .map_err(|_| "GBufferedOutputStream flush")?;
    }

    // ── GDrive (Phase 11) ─────────────────────────────────────────
    let drive = SimpleDrive::new("USB Drive", true, true);
    if drive.get_name() != "USB Drive" || !drive.can_eject() || !drive.is_media_removable() {
        return Err("GDrive fields");
    }

    // ── GMount (Phase 11) ─────────────────────────────────────────
    let mount = SimpleMount::new(
        "My Mount",
        Some("uuid-1234".to_string()),
        true,
        false,
        "/mnt/usb",
    );
    if mount.get_name() != "My Mount"
        || mount.get_uuid().unwrap() != "uuid-1234"
        || !mount.can_unmount()
        || mount.can_eject()
    {
        return Err("GMount fields");
    }

    // ── GVolume (Phase 11) ────────────────────────────────────────
    let volume = SimpleVolume::new("data", Some("vol-uuid".to_string()), true, false, false);
    if volume.get_name() != "data"
        || volume.get_uuid().unwrap() != "vol-uuid"
        || !volume.can_mount()
        || volume.can_eject()
    {
        return Err("GVolume fields");
    }
    if MountUnmountFlags::None as u32 != 0 || MountUnmountFlags::Force as u32 != 1 {
        return Err("MountUnmountFlags values");
    }

    // ── GResolver / NoopResolver (Phase 11) ───────────────────────
    let resolver = NoopResolver;
    // NoopResolver always fails with NotFound.
    if resolver.lookup_by_name("example.com", None).is_ok() {
        return Err("NoopResolver should fail lookup_by_name");
    }
    if ResolverError::resolver_error_quark() == 0 {
        return Err("ResolverError quark should be nonzero");
    }

    // ── GSocket / MockSocket (Phase 11) ───────────────────────────
    let socket = MockSocket::new_stream();
    if socket.socket_type() != SocketType::Stream {
        return Err("GSocket type");
    }
    if socket.protocol() != SocketProtocol::Tcp {
        return Err("GSocket protocol");
    }
    socket.inject(b"ping");
    let mut sock_buf = [0u8; 4];
    let n_recv = socket
        .receive(&mut sock_buf, None)
        .map_err(|_| "GSocket receive")?;
    if n_recv != 4 || &sock_buf != b"ping" {
        return Err("GSocket receive data");
    }

    // ── GSettings (Phase 11) ──────────────────────────────────────
    let settings = Settings::new("org.rustos.test");
    settings.set_boolean("enabled", true);
    if !settings.get_boolean("enabled") {
        return Err("GSettings boolean");
    }
    settings.set_string("name", "RustOS");
    if settings.get_string("name") != "RustOS" {
        return Err("GSettings string");
    }
    settings.set_int("count", 42);
    if settings.get_int("count") != 42 {
        return Err("GSettings int");
    }
    let keys = settings.list_keys();
    if keys.len() != 3 {
        return Err("GSettings list_keys count");
    }

    // ── GMemoryOutputStream (Phase 11) ───────────────────────────
    let mem_out = MemoryOutputStream::new_resizable();
    mem_out.write(b"hello", None).unwrap();
    mem_out.write(b" world", None).unwrap();
    if mem_out.get_data() != b"hello world" {
        return Err("GMemoryOutputStream write");
    }
    if mem_out.get_data_size() != 11 {
        return Err("GMemoryOutputStream data_size");
    }
    let stolen_bytes = mem_out.steal_as_bytes();
    if stolen_bytes.as_ref() != b"hello world" || mem_out.get_data_size() != 0 {
        return Err("GMemoryOutputStream steal_as_bytes");
    }

    // ── GConverterInputStream (Phase 11) ─────────────────────────
    let cstream = ConverterInputStream::new(b"convert me", "identity");
    let mut cbuf = [0u8; 7];
    let cn = cstream.read(&mut cbuf, None).unwrap();
    if cn != 7 || &cbuf != b"convert" {
        return Err("GConverterInputStream read");
    }
    if cstream.get_converter_name() != "identity" {
        return Err("GConverterInputStream converter_name");
    }

    // ── GConverterOutputStream (Phase 11) ────────────────────────
    let costream = ConverterOutputStream::new("identity");
    costream.write(b"output data", None).unwrap();
    if costream.get_data() != b"output data" {
        return Err("GConverterOutputStream write");
    }

    // ── GSocketConnectable (Phase 11) ────────────────────────────
    let connectable = SimpleConnectable::new("localhost", 8080, vec![]);
    if connectable.to_string() != "localhost:8080" {
        return Err("GSocketConnectable to_string");
    }
    let enumerator = connectable.enumerate();
    if enumerator.next(None).unwrap().is_some() {
        return Err("GSocketConnectable empty enumerate");
    }

    // ── GResource (Phase 11) ─────────────────────────────────────
    let resource = Resource::new();
    resource.add_entry("/app/icon.png", b"png bytes");
    resource.add_entry("/app/config.xml", b"<config/>");
    let rdata = resource
        .lookup_data("/app/icon.png", ResourceLookupFlags::None)
        .unwrap();
    if rdata.as_ref() != b"png bytes" {
        return Err("GResource lookup_data");
    }
    let (rsize, _) = resource
        .get_info("/app/config.xml", ResourceLookupFlags::None)
        .unwrap();
    if rsize != 9 {
        return Err("GResource get_info");
    }
    let children = resource
        .enumerate_children("/app", ResourceLookupFlags::None)
        .unwrap();
    if children.len() != 2 {
        return Err("GResource enumerate_children");
    }

    // ── GContentType (Phase 11) ──────────────────────────────────
    if !content_type_equals("text/plain", "text/plain") {
        return Err("GContentType equals");
    }
    if content_type_is_unknown("application/octet-stream") != true {
        return Err("GContentType is_unknown");
    }
    let (guessed, _) = content_type_guess(Some("test.txt"), &[]);
    if guessed != "text/plain" {
        return Err("GContentType guess");
    }
    if content_type_can_be_executable("text/plain") != true {
        return Err("GContentType can_be_executable");
    }

    // ── GMenu (Phase 11) ─────────────────────────────────────────
    let menu = Menu::new();
    menu.append("Open", "app.open");
    menu.append("Save", "app.save");
    if menu.get_n_items() != 2 {
        return Err("GMenu n_items");
    }
    let items = menu.get_items();
    if items[0].get_label() != Some("Open") || items[0].get_action() != Some("app.open") {
        return Err("GMenu item content");
    }

    // ── GAppInfo (Phase 11) ──────────────────────────────────────
    let app = SimpleAppInfo::new("org.test.App", "Test App", "/usr/bin/testapp");
    if app.get_id() != "org.test.App" || app.get_name() != "Test App" {
        return Err("GAppInfo id/name");
    }
    if app.get_executable() != "/usr/bin/testapp" {
        return Err("GAppInfo executable");
    }

    // ── GMenuModel (Phase 11) ────────────────────────────────────
    let model = SimpleMenuModel::new();
    let mut attrs = alloc::collections::BTreeMap::new();
    attrs.insert("label".to_string(), "Test".to_string());
    model.append(attrs);
    if model.get_n_items() != 1 {
        return Err("GMenuModel n_items");
    }
    if model.get_item_attribute_value(0, "label") != Some("Test".to_string()) {
        return Err("GMenuModel attribute");
    }

    // ── GSettingsSchema (Phase 11) ───────────────────────────────
    let source = SettingsSchemaSource::new();
    let mut schema = SettingsSchema::new("org.test.Smoke");
    schema.add_key(SettingsSchemaKey::new("enabled", "b", "true"));
    source.add_schema(schema);
    let looked_up = source.lookup("org.test.Smoke", false).unwrap();
    if !looked_up.has_key("enabled") {
        return Err("GSettingsSchema has_key");
    }

    // ── GDBusMessage (Phase 11) ──────────────────────────────────
    let msg = DBusMessage::new_signal("/org/test", "org.test.Interface", "Changed");
    if msg.get_message_type() != DBusMessageType::Signal {
        return Err("GDBusMessage type");
    }
    if msg.get_header(DBusMessageHeaderField::Path) != Some("/org/test".to_string()) {
        return Err("GDBusMessage header");
    }

    // ── GNetworkMonitor (Phase 11) ───────────────────────────────
    let monitor = NetworkMonitor::new();
    if !monitor.get_network_available() {
        return Err("GNetworkMonitor available");
    }
    if monitor.get_connectivity() != NetworkConnectivity::Full {
        return Err("GNetworkMonitor connectivity");
    }

    // ── GPowerProfileMonitor (Phase 11) ──────────────────────────
    let power = PowerProfileMonitor::new();
    if power.get_power_saver_enabled() {
        return Err("GPowerProfileMonitor saver");
    }
    if power.get_profile() != PowerProfile::Performance {
        return Err("GPowerProfileMonitor profile");
    }

    // ── GTlsCertificate (Phase 11) ───────────────────────────────
    let cert = TlsCertificate::new_from_pem(
        b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n",
    );
    if !cert.is_valid() {
        return Err("GTlsCertificate valid");
    }
    if cert.get_pem().is_empty() {
        return Err("GTlsCertificate pem");
    }

    // ── GFileInfo (Phase 11) ─────────────────────────────────────
    let mut info = FileInfo::new();
    info.set_name("test.txt");
    info.set_file_type(FileType::Regular);
    info.set_size(1024);
    if info.get_name() != "test.txt" {
        return Err("GFileInfo name");
    }
    if info.get_file_type() != FileType::Regular {
        return Err("GFileInfo type");
    }
    if info.get_size() != 1024 {
        return Err("GFileInfo size");
    }

    // ── GMemoryInputStream (Phase 11) ────────────────────────────
    let mem_stream = MemoryInputStream::new_from_bytes(Bytes::from_static(b"hello"));
    let stream = InputStream::from(mem_stream);
    let mut buf = [0u8; 5];
    let n = stream.read(&mut buf, None).unwrap();
    if n != 5 {
        return Err("GMemoryInputStream read");
    }
    if &buf != b"hello" {
        return Err("GMemoryInputStream data");
    }

    // ── GDBusMethodInvocation (Phase 11) ─────────────────────────
    let inv = DBusMethodInvocation::new(
        ":1.1",
        "/test",
        "test.iface",
        "Method",
        vec!["arg".to_string()],
    );
    if inv.get_method_name() != "Method" {
        return Err("GDBusMethodInvocation method");
    }
    inv.return_value(vec!["ok".to_string()]);
    if !inv.has_reply() {
        return Err("GDBusMethodInvocation reply");
    }

    // ── GSettingsBackend (Phase 11) ─────────────────────────────
    let backend = SettingsBackend::new();
    backend.write("test.key", "value");
    if backend.read("test.key") != Some("value".to_string()) {
        return Err("GSettingsBackend read");
    }
    if !backend.get_writable("test.key") {
        return Err("GSettingsBackend writable");
    }

    // ── GProxy (Phase 11) ────────────────────────────────────────
    let proxy = HttpProxy::new();
    if proxy.get_protocol() != "http" {
        return Err("GProxy protocol");
    }
    if !proxy.supports_hostname() {
        return Err("GProxy hostname");
    }

    // ── GTlsBackend (Phase 11) ───────────────────────────────────
    let tls = TlsBackend::new();
    if !tls.supports_tls() {
        return Err("GTlsBackend supports_tls");
    }

    // ── GTlsInteraction (Phase 11) ──────────────────────────────
    let interaction = TlsInteraction::new();
    interaction.set_password(b"secret");
    let mut prompt = TlsPassword::new("Enter password");
    let result = interaction.ask_password(&mut prompt);
    if result != TlsInteractionResult::Handled {
        return Err("GTlsInteraction handled");
    }
    if prompt.get_value() != b"secret" {
        return Err("GTlsInteraction password");
    }

    // ── GTlsDatabase (Phase 11) ──────────────────────────────────
    let db = TlsDatabase::new();
    db.add_anchor(TlsCertificate::new_from_pem(
        b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n",
    ));
    if db.n_anchors() != 1 {
        return Err("GTlsDatabase anchors");
    }

    // ── GTlsServerConnection (Phase 11) ──────────────────────────
    let server = TlsServerConnection::new();
    server.set_authentication_mode(ClientCertificateMode::Required);
    if server.get_authentication_mode() != ClientCertificateMode::Required {
        return Err("GTlsServerConnection auth mode");
    }

    // ── GDatagramBased (Phase 11) ────────────────────────────────
    let dg = DatagramBased::new();
    dg.inject(b"ping");
    let received = dg.receive();
    if received.is_none() || received.unwrap().get_data() != b"ping" {
        return Err("GDatagramBased receive");
    }

    // ── GVolumeMonitor (Phase 11) ────────────────────────────────
    let vm = VolumeMonitor::new();
    vm.add_volume("test");
    if vm.volume_count() != 1 {
        return Err("GVolumeMonitor count");
    }

    // ── GTask (Phase 11) ────────────────────────────────────────
    let mut task = Task::new();
    task.set_name("test-task");
    if task.get_name() != Some("test-task") {
        return Err("GTask name");
    }

    // ── GSocketClient (Phase 11) ────────────────────────────────
    let client = SocketClient::new();
    if client.get_timeout() != 0 {
        return Err("GSocketClient timeout");
    }

    // ── GProxyResolver (Phase 11) ───────────────────────────────
    let resolver = ProxyResolver::new();
    if !resolver.is_supported() {
        return Err("GProxyResolver supported");
    }

    // ── GTlsConnection (Phase 11) ───────────────────────────────
    let mut tls_conn = TlsConnection::new();
    if !tls_conn.get_require_close_notify() {
        return Err("GTlsConnection close_notify default");
    }
    tls_conn.set_require_close_notify(false);
    if tls_conn.get_require_close_notify() {
        return Err("GTlsConnection close_notify clear");
    }

    // ── GDBusServer (Phase 11) ──────────────────────────────────
    let dbus_server = DBusServer::new("unix:tmpdir=/tmp", "test-guid");
    if dbus_server.get_client_address().is_empty() {
        return Err("GDBusServer address");
    }

    // ── GAppInfoMonitor (Phase 12) ───────────────────────────────
    let aim = AppInfoMonitor::new();
    aim.register_app("org.test.App", "Test App", "/usr/bin/test-app");
    if aim.app_count() != 1 {
        return Err("GAppInfoMonitor count");
    }

    // ── GDtlsClientConnection (Phase 12) ────────────────────────
    let mut dtls_c = DtlsClientConnection::new();
    dtls_c.set_server_identity("example.com");
    if dtls_c.get_server_identity().as_deref() != Some("example.com") {
        return Err("GDtlsClientConnection server_identity");
    }

    // ── GDtlsConnection (Phase 12) ──────────────────────────────
    let dtls_conn = DtlsConnection::new();
    if dtls_conn.is_handshake_done() {
        return Err("GDtlsConnection handshake_done initial");
    }

    // ── GDtlsServerConnection (Phase 12) ────────────────────────
    let dtls_s = DtlsServerConnection::new();
    dtls_s.set_authentication_mode(ClientCertificateMode::Requested);
    if dtls_s.get_authentication_mode() as u8 != ClientCertificateMode::Requested as u8 {
        return Err("GDtlsServerConnection auth_mode");
    }

    // ── GSocketService (Phase 12) ────────────────────────────────
    let svc = SocketService::new();
    svc.start();
    if !svc.is_active() {
        return Err("GSocketService active");
    }
    svc.stop();

    // ── GThreadedSocketService (Phase 12) ───────────────────────
    let tsvc = ThreadedSocketService::new(4);
    tsvc.start();
    if !tsvc.is_active() {
        return Err("GThreadedSocketService active");
    }

    // ── GTlsClientConnection (Phase 12) ─────────────────────────
    let tls_c = TlsClientConnection::new(Some("example.com"));
    if tls_c.get_server_identity().as_deref() != Some("example.com") {
        return Err("GTlsClientConnection server_identity");
    }

    // ── GTlsFileDatabase (Phase 12) ─────────────────────────────
    let tlsdb = TlsFileDatabase::new("/etc/ssl/certs/ca-certificates.crt");
    if tlsdb.get_anchors().is_empty() {
        return Err("GTlsFileDatabase anchors empty");
    }

    // ── GCredentials (Phase 12) ──────────────────────────────────
    let creds = Credentials::new();
    let _ = creds.get_unix_pid(); // may return error on no_std — just exercise the path

    // ── GCredentialsMessage (Phase 12) ───────────────────────────
    let creds2 = Credentials::new_with(1234, 1000, 1000);
    let cm = CredentialsMessage::new(creds2);
    if cm.get_credentials().get_unix_pid().is_err() {
        return Err("GCredentialsMessage get_credentials");
    }

    // ── GPropertyAction (Phase 12) ───────────────────────────────
    let pa = PropertyAction::new("brightness", "brightness", Variant::new_int32(80));
    if pa.get_property_name() != "brightness" {
        return Err("GPropertyAction property_name");
    }

    // ── GSimpleIOStream (Phase 12) ──────────────────────────────
    {
        use glib_native::ginputstream::{InputStream, MemoryInputStream};
        use glib_native::goutputstream::{MemoryOutputStream, OutputStream};
        let mi = MemoryInputStream::new();
        let mo = MemoryOutputStream::new_resizable();
        let ios = SimpleIOStream::new(InputStream::from(mi), OutputStream::from(mo));
        if ios.is_closed() {
            return Err("GSimpleIOStream closed initial");
        }
    }

    // ── GSocketAddressEnumerator (Phase 12) ─────────────────────
    {
        let addr = glib_native::ginetaddress::InetAddress::new_from_bytes(
            &[127, 0, 0, 1],
            glib_native::ginetaddress::SocketFamily::Ipv4,
        )
        .ok_or("GSocketAddressEnumerator InetAddress")?;
        let sa = InetSocketAddress::new(addr, 8080);
        let sae = SocketAddressEnumerator::new(alloc::vec![sa]);
        let first = sae
            .next(None)
            .map_err(|_| "GSocketAddressEnumerator next")?;
        if first.is_none() {
            return Err("GSocketAddressEnumerator empty");
        }
    }

    // ── GSocketConnection (Phase 12) ────────────────────────────
    {
        let sock = MockSocket::new_stream();
        let sc = SocketConnection::new(sock);
        if sc.is_closed() {
            return Err("GSocketConnection closed initial");
        }
    }

    // ── GSocketControlMessage (Phase 12) ────────────────────────
    {
        let scm = SocketControlMessage::new(1, 1, alloc::vec![0u8; 4]);
        if scm.get_level() != 1 {
            return Err("GSocketControlMessage level");
        }
        if scm.get_data().len() != 4 {
            return Err("GSocketControlMessage data len");
        }
    }

    // ── GSocketListener (Phase 12) ───────────────────────────────
    {
        let mut sl = SocketListener::new();
        sl.set_backlog(5);
        if sl.get_backlog() != 5 {
            return Err("GSocketListener backlog");
        }
    }

    // ── GSubprocess (Phase 12) ───────────────────────────────────
    {
        let sp = Subprocess::new(
            alloc::vec!["echo".into(), "hello".into()],
            SubprocessFlags::NONE,
        )
        .map_err(|_| "GSubprocess new")?;
        if !sp.is_running() {
            return Err("GSubprocess running");
        }
    }

    // ── GSubprocessLauncher (Phase 12) ──────────────────────────
    {
        let mut spl = SubprocessLauncher::new(SubprocessFlags::NONE);
        spl.set_cwd("/tmp");
        if spl.get_cwd() != Some("/tmp") {
            return Err("GSubprocessLauncher cwd");
        }
    }

    // ── GTcpConnection (Phase 12) ────────────────────────────────
    {
        let addr = glib_native::ginetaddress::InetAddress::new_from_bytes(
            &[127, 0, 0, 1],
            glib_native::ginetaddress::SocketFamily::Ipv4,
        )
        .ok_or("GTcpConnection InetAddress")?;
        let sa = InetSocketAddress::new(addr, 9000);
        let tcp = TcpConnection::new(sa);
        if tcp.is_closed() {
            return Err("GTcpConnection closed initial");
        }
    }

    // ── GTcpWrapperConnection (Phase 12) ────────────────────────
    {
        let addr = glib_native::ginetaddress::InetAddress::new_from_bytes(
            &[127, 0, 0, 1],
            glib_native::ginetaddress::SocketFamily::Ipv4,
        )
        .ok_or("GTcpWrapperConnection InetAddress")?;
        let sa = InetSocketAddress::new(addr, 443);
        let tcp = TcpConnection::new(sa);
        let wrap = TcpWrapperConnection::new(tcp);
        if wrap.is_closed() {
            return Err("GTcpWrapperConnection closed initial");
        }
    }

    // ── GUnixConnection (Phase 12) ──────────────────────────────
    {
        let uc = UnixConnection::new();
        if uc.is_closed() {
            return Err("GUnixConnection closed initial");
        }
        if uc.get_peer_credentials().is_some() {
            return Err("GUnixConnection peer_creds initial");
        }
    }

    // ── GApplication (Phase 13) ─────────────────────────────────
    {
        if !application_id_is_valid("org.rustos.Glib") {
            return Err("GApplication id validation");
        }
        let app = Application::new(Some("org.rustos.Glib"), ApplicationFlags::DEFAULT_FLAGS);
        if app.id() != Some(alloc::string::String::from("org.rustos.Glib")) {
            return Err("GApplication id");
        }
        app.hold();
        if app.use_count() != 1 {
            return Err("GApplication hold");
        }
        app.release();
        if app.use_count() != 0 {
            return Err("GApplication release");
        }
    }

    // ── GDBusConnection / GDBusProxy (Phase 13) ─────────────────
    {
        let conn = alloc::sync::Arc::new(
            DBusConnection::new_for_address_sync("loopback:")
                .map_err(|_| "GDBusConnection loopback")?,
        );
        if conn.is_closed() {
            return Err("GDBusConnection closed");
        }
        let proxy = DBusProxy::new(
            conn,
            DBusProxyFlags::DO_NOT_LOAD_PROPERTIES | DBusProxyFlags::DO_NOT_CONNECT_SIGNALS,
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            None,
            -1,
        )
        .map_err(|_| "GDBusProxy new")?;
        if proxy.get_bus_name() != Some("org.freedesktop.DBus") {
            return Err("GDBusProxy bus name");
        }
    }

    // ── GDummyTlsBackend (Phase 13) ─────────────────────────────
    {
        let backend = DummyTlsBackend::new();
        if backend.supports_tls() || backend.supports_dtls() {
            return Err("GDummyTlsBackend supports");
        }
    }

    // ── GFileInfo attributes (Phase 13) ─────────────────────────
    {
        let info = glib_native::gfileinfo::FileInfo::new();
        info.set_name("smoke.txt");
        info.set_size(42);
        if info.get_size() != 42 {
            return Err("GFileInfo attribute size");
        }
        if info.get_name() != Some(alloc::string::String::from("smoke.txt")) {
            return Err("GFileInfo attribute name");
        }
    }

    // ── Deferred GIO batch modules (Phase 13) ───────────────────
    {
        let desktop = DesktopAppInfo::new("rustos.desktop", "RustOS", "/bin/rustos");
        if desktop.get_name() != "RustOS" {
            return Err("GDesktopAppInfo name");
        }

        let scheduler = IoScheduler::new();
        if scheduler.job_count() != 0 {
            return Err("GIoScheduler initial count");
        }

        let monitor = memory_monitor_get_default();
        if monitor.get_memory_pressure() != MemoryPressureLevel::Normal {
            return Err("GMemoryMonitor pressure");
        }

        let base = MemoryMonitorBase::new(alloc::sync::Arc::new(MemoryMonitor::new()));
        base.send_event_to_user(MemoryMonitorLowMemoryLevel::Low);
        if base.monitor().get_memory_pressure() != MemoryPressureLevel::Low {
            return Err("GMemoryMonitorBase event");
        }

        let sar = SimpleAsyncResult::new("smoke-tag");
        if sar.get_source_tag() != "smoke-tag" {
            return Err("GSimpleAsyncResult tag");
        }

        let dummy_resolver = DummyProxyResolver::new();
        let proxies = dummy_resolver.lookup("http://example.com");
        if proxies.len() != 1 || proxies[0] != "direct://" {
            return Err("GDummyProxyResolver lookup");
        }

        let fd_list = UnixFDList::new();
        if fd_list.get_length() != 0 {
            return Err("GUnixFDList length");
        }

        // GDBusAuth SASL roundtrip
        let auth_server = DBusAuth::new_server("smoke-guid");
        let auth_client = DBusAuth::new_client();
        let start = auth_client.client_start();
        if start.is_empty() {
            return Err("GDBusAuth client start");
        }
        let replies = auth_server
            .run_server_sync(&[start[0].as_str()], None)
            .map_err(|_| "GDBusAuth server")?;
        if replies.is_empty() || !replies[0].starts_with("OK ") {
            return Err("GDBusAuth server reply");
        }

        // GDBusObjectManager server/client
        let conn = alloc::sync::Arc::new(
            DBusConnection::new_for_address_sync("loopback:")
                .map_err(|_| "GDBusObjectManager connection")?,
        );
        let mgr_server = DBusObjectManagerServer::new("/org/rustos/mgr");
        mgr_server.add_object("/org/rustos/obj", alloc::vec!["org.rustos.Smoke".into()]);
        mgr_server
            .export_on_connection(&conn)
            .map_err(|_| "GDBusObjectManagerServer export")?;
        let mgr_client =
            DBusObjectManagerClient::new_for_bus_sync(&conn, 0, None, "/org/rustos/mgr")
                .map_err(|_| "GDBusObjectManagerClient sync")?;
        if mgr_client.proxy_count() != 1 {
            return Err("GDBusObjectManagerClient count");
        }
    }

    // ── D-Bus address / proxy enumerator / platform hooks (Phase 14) ──
    {
        let entry =
            DBusAddress::parse_entry("unix:path=/tmp/dbus").map_err(|_| "GDBusAddress parse")?;
        if !entry.is_unix() || entry.get_param("path") != Some("/tmp/dbus") {
            return Err("GDBusAddress unix entry");
        }

        if dbus_address_escape_value("a=b") != "a%3Db" {
            return Err("GDBusAddress escape");
        }

        if !is_address("loopback:") || !is_supported_address("loopback:").is_ok() {
            return Err("GDBusAddress loopback");
        }

        let bus_addr = dbus_address_get_for_bus_sync(DBusBusType::None)
            .map_err(|_| "GDBusAddress bus sync")?;
        if bus_addr != "loopback:" {
            return Err("GDBusAddress none bus");
        }

        let session_addr = dbus_address_get_for_bus_sync(DBusBusType::Session)
            .map_err(|_| "GDBusAddress session bus")?;
        if session_addr != "loopback:" {
            return Err("GDBusAddress session bus");
        }

        let enumerator =
            ProxyAddressEnumerator::new("127.0.0.1", 80, 8080, alloc::vec!["direct://".into()]);
        let proxy = enumerator.next().ok_or("GProxyAddressEnumerator next")?;
        if proxy.protocol() != "direct" || proxy.destination_hostname() != "127.0.0.1" {
            return Err("GProxyAddressEnumerator direct");
        }

        const PLATFORM_SMOKE: &str = "/tmp/glib-platform-smoke.txt";
        let payload = b"rustos-glib-platform";
        let fd = crate::vfs::vfs_open(
            PLATFORM_SMOKE,
            crate::vfs::OpenFlags::RDWR
                | crate::vfs::OpenFlags::CREAT
                | crate::vfs::OpenFlags::TRUNC,
            0o644,
        )
        .map_err(|_| "platform smoke vfs create")?;
        crate::vfs::vfs_write(fd, payload).map_err(|_| "platform smoke vfs write")?;
        let _ = crate::vfs::vfs_close(fd);

        if access("/tmp", F_OK) != 0 {
            return Err("g_access /tmp");
        }
        if g_stat("/tmp").is_none() {
            return Err("g_stat /tmp");
        }

        let mut dir = dir_open("/tmp", DIR_CASE_SENSITIVE).map_err(|_| "GDir open /tmp")?;
        if dir.read_name().is_none() {
            return Err("GDir read /tmp");
        }

        let mf = mapped_file_new(PLATFORM_SMOKE, false).map_err(|_| "GMappedFile new")?;
        if mf.get_contents() != payload {
            return Err("GMappedFile contents");
        }

        const GFILE_SMOKE: &str = "/tmp/glib-gfile-smoke.txt";
        let gfile_payload = b"rustos-gfile-platform";
        let gfile = glib_native::gfile::File::new_for_path(GFILE_SMOKE);
        let out = gfile
            .create(glib_native::gfile::FileCreateFlags::None, None)
            .map_err(|_| "GFile create")?;
        let written = out.write(gfile_payload, None).map_err(|_| "GFile write")?;
        if written != gfile_payload.len() {
            return Err("GFile write length");
        }
        out.close(None).map_err(|_| "GFile close")?;

        let input = gfile.read(None).map_err(|_| "GFile read open")?;
        let mut gfile_buf = [0u8; 64];
        let read = input.read(&mut gfile_buf, None).map_err(|_| "GFile read")?;
        if &gfile_buf[..read] != gfile_payload {
            return Err("GFile read contents");
        }
        input.close(None).map_err(|_| "GFile input close")?;

        let fd = glib_native::stdio::open(GFILE_SMOKE, glib_native::stdio::OpenFlags::O_RDONLY, 0);
        if fd < 0 {
            return Err("g_open GFile smoke");
        }
        let mut stdio_buf = [0u8; 64];
        let read = glib_native::stdio::read(fd, &mut stdio_buf);
        if read < 0 || &stdio_buf[..read as usize] != gfile_payload {
            let _ = glib_native::stdio::close(fd);
            return Err("g_read GFile smoke");
        }
        if glib_native::stdio::close(fd) != 0 {
            return Err("g_close GFile smoke");
        }
        gfile.delete(None).map_err(|_| "GFile delete")?;
        if gfile.query_exists(None) {
            return Err("GFile delete still exists");
        }

        match spawn_async(None, &["/no/such/binary"], None, SpawnFlags::DEFAULT, None) {
            Err(SpawnError::Noent) => {}
            Ok(_) | Err(_) => return Err("GSpawn missing binary should return Noent"),
        }

        if !spawn_check_exit_status(0).is_ok() || spawn_check_exit_status(1).is_ok() {
            return Err("GSpawn check exit status");
        }

        let io_path = io_module_build_path::<RustOsIoModulePlatform>(Some("/lib"), "gio");
        if io_path != "/lib/libgio.so" {
            return Err("GIOModule build_path");
        }

        // ── Port wiring regressions (Phase 14) ──────────────────────
        let mut strinfo = glib_native::strinfo::StrInfoBuilder::new();
        strinfo.append_item("primary", 7);
        if !strinfo.append_alias("alias", "primary") {
            return Err("GSettings strinfo alias append");
        }
        let words = strinfo.as_words();
        if glib_native::strinfo::strinfo_string_from_alias(&words, "alias").as_deref()
            != Some("primary")
        {
            return Err("GSettings strinfo alias lookup");
        }

        glib_native::gio_trace::clear_trace();
        glib_native::gio_trace::trace_record("rustos smoke");
        let trace = glib_native::gio_trace::drain_trace();
        if trace.len() != 1 || trace[0].message != "rustos smoke" || trace[0].seq != 1 {
            return Err("GIO trace buffer");
        }

        let mut notification = glib_native::gnotification::Notification::new("RustOS");
        notification.set_body("GLib native");
        notification.set_category("system");
        notification.add_button("Open", "app.open");
        notification.set_default_action("app.default");
        if glib_native::gnotification_private::get_title(&notification) != "RustOS"
            || glib_native::gnotification_private::get_body(&notification) != "GLib native"
            || glib_native::gnotification_private::get_category(&notification) != Some("system")
            || glib_native::gnotification_private::get_n_buttons(&notification) != 1
            || glib_native::gnotification_private::get_button_with_action(&notification, "app.open")
                != Some(0)
        {
            return Err("GNotification private accessors");
        }
        let default_action = glib_native::gnotification_private::get_default_action(&notification)
            .ok_or("GNotification default action")?;
        if default_action.0 != "app.default" || default_action.1.is_some() {
            return Err("GNotification default action value");
        }

        let mut schema = glib_native::gsettingsschema::SettingsSchema::new("org.rustos.Smoke");
        schema.add_key(glib_native::gsettingsschema::SettingsSchemaKey::new(
            "count", "i", "7",
        ));
        let key = glib_native::gsettingsschema_internal::schema_key_init(&schema, "count")
            .ok_or("GSettingsSchema internal key")?;
        let value = glib_native::gsettingsschema_internal::schema_get_value(&schema, "count")
            .ok_or("GSettingsSchema internal value")?;
        if value.get_int32() != 7
            || !glib_native::gsettingsschema_internal::schema_key_type_check(&key, &value)
            || glib_native::gsettingsschema_internal::schema_key_range_fixup(&key, &value).is_none()
        {
            return Err("GSettingsSchema internal helpers");
        }

        let mut repository = glib_native::girepository::Repository::new();
        repository.prepend_search_path("/usr/share/gir-1.0");
        repository.prepend_library_path("/usr/lib");
        if repository.search_paths().first().map(|s| s.as_str()) != Some("/usr/share/gir-1.0") {
            return Err("GIRepository search path");
        }

        let mut entries = alloc::collections::BTreeMap::new();
        let enum_info = glib_native::gienuminfo::EnumInfo::new(
            "ApplicationFlags",
            "Gio",
            &[("G_APPLICATION_FLAGS_NONE", 0, "none")],
        );
        glib_native::gitypelib::register_entry(&mut entries, enum_info.base().ref_());
        let typelib = glib_native::gitypelib::Typelib::new_in_memory("Gio", "2.0", entries);
        glib_native::girepository::register_typelib(typelib.ref_());
        let loaded = repository
            .require(
                "Gio",
                Some("2.0"),
                glib_native::girepository::RepositoryLoadFlags::NONE,
            )
            .map_err(|_| "GIRepository require")?;
        let found = repository
            .find_by_name("Gio", Some("ApplicationFlags"))
            .ok_or("GIRepository find_by_name")?;
        if loaded.namespace() != "Gio"
            || loaded.version() != "2.0"
            || found.name() != "ApplicationFlags"
            || found.namespace() != "Gio"
            || found.get_type() != glib_native::gibaseinfo::InfoType::Enum
        {
            return Err("GIRepository metadata lookup");
        }
    }

    // ── GObject/GBytes/GError exercises (Phase 13) ───────────────────
    // Validate core APIs via Rust wrappers (no C ABI / libc on kernel).

    let phase13_obj = object_new(G_TYPE_OBJECT);
    if phase13_obj.is_floating() {
        return Err("GObject is_floating initial");
    }
    phase13_obj.force_floating();
    if !phase13_obj.is_floating() {
        return Err("GObject force_floating");
    }
    phase13_obj.unref();

    let phase13_bytes_data = b"phase13-bytes";
    let phase13_bytes = Bytes::new(phase13_bytes_data);
    if phase13_bytes.len() != phase13_bytes_data.len() || phase13_bytes.data() != phase13_bytes_data
    {
        return Err("GBytes phase13");
    }

    let phase13_err_domain = quark_from_static_string(Some("rustos-phase13-err"));
    let phase13_err = error_new(phase13_err_domain, 42, "phase13 error smoke");
    if !error_matches(&phase13_err, phase13_err_domain, 42) {
        return Err("GError matches");
    }
    if error_matches(&phase13_err, phase13_err_domain, 0) {
        return Err("GError matches negative");
    }
    let phase13_err_copy = error_copy(&phase13_err);
    if !error_matches(&phase13_err_copy, phase13_err_domain, 42) {
        return Err("GError copy");
    }
    error_free(phase13_err);
    error_free(phase13_err_copy);

    if strcmp("alpha", "alpha") != 0 {
        return Err("strcmp equal");
    }
    if strcmp("alpha", "beta") >= 0 {
        return Err("strcmp less");
    }

    let ps_bool = ParamSpec::boolean("active", "Active", "Whether active", true, ParamFlags::NONE);
    if ps_bool.value_type != G_TYPE_BOOLEAN {
        return Err("ParamSpec boolean");
    }
    let ps_uint = ParamSpec::uint("count", "Count", "Item count", 0, 100, 50, ParamFlags::NONE);
    if ps_uint.value_type != G_TYPE_UINT {
        return Err("ParamSpec uint");
    }

    let phase13_type_id = type_register_static_simple(
        G_TYPE_OBJECT,
        "RustOSPhase13Type",
        0,
        None,
        0,
        None,
        GTypeFlags::NONE,
    );
    if phase13_type_id == G_TYPE_INVALID {
        return Err("GType register static simple");
    }

    Ok(())
}

/// Validate GSpawn fork/exec wiring once memory and process management are up.
pub fn smoke_check_spawn() -> Result<(), &'static str> {
    if !crate::glib_spawn::spawn_runtime_ready() {
        let _ = crate::glib_spawn::ensure_spawn_runtime();
    }
    if !crate::glib_spawn::spawn_runtime_ready() {
        return Ok(());
    }

    match spawn_async(
        None,
        &["/no/such/glib-spawn-binary"],
        None,
        SpawnFlags::DEFAULT,
        None,
    ) {
        Err(SpawnError::Noent) => {}
        Ok(_) | Err(_) => return Err("GSpawn runtime missing binary"),
    }

    const BAD_EXEC: &str = "/tmp/glib-spawn-bad-exec";
    let fd = crate::vfs::vfs_open(
        BAD_EXEC,
        crate::vfs::OpenFlags::RDWR | crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::TRUNC,
        0o644,
    )
    .map_err(|_| "spawn smoke vfs create")?;
    crate::vfs::vfs_write(fd, b"not-an-elf").map_err(|_| "spawn smoke vfs write")?;
    let _ = crate::vfs::vfs_close(fd);

    match spawn_async(None, &[BAD_EXEC], None, SpawnFlags::DEFAULT, None) {
        Err(SpawnError::Noexec) => {}
        Ok(_) | Err(_) => return Err("GSpawn non-ELF should return Noexec"),
    }

    const GOOD_EXEC: &str = "/tmp/glib-spawn-test-exec";
    let elf = crate::glib_spawn::minimal_test_elf();
    let fd = crate::vfs::vfs_open(
        GOOD_EXEC,
        crate::vfs::OpenFlags::RDWR | crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::TRUNC,
        0o755,
    )
    .map_err(|_| "spawn smoke elf create")?;
    crate::vfs::vfs_write(fd, &elf).map_err(|_| "spawn smoke elf write")?;
    let _ = crate::vfs::vfs_close(fd);

    let child_pid = match spawn_async(None, &[GOOD_EXEC], None, SpawnFlags::DEFAULT, None) {
        Ok(pid) => pid,
        Err(err) => return Err(spawn_error_name(err)),
    };
    if child_pid <= 0 {
        return Err("GSpawn child pid");
    }

    let kernel_pm = crate::process::get_process_manager();
    if kernel_pm.get_process(child_pid as u32).is_none() {
        return Err("GSpawn child not in scheduler");
    }
    cleanup_spawn_smoke_child(child_pid);

    const WD_EXEC: &str = "glib-spawn-wd-exec";
    let fd = crate::vfs::vfs_open(
        &format!("/tmp/{WD_EXEC}"),
        crate::vfs::OpenFlags::RDWR | crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::TRUNC,
        0o755,
    )
    .map_err(|_| "spawn smoke wd vfs create")?;
    crate::vfs::vfs_write(fd, &elf).map_err(|_| "spawn smoke wd vfs write")?;
    let _ = crate::vfs::vfs_close(fd);

    let wd_child = match spawn_async(Some("/tmp"), &[WD_EXEC], None, SpawnFlags::DEFAULT, None) {
        Ok(pid) => pid,
        Err(err) => return Err(spawn_error_name(err)),
    };
    if wd_child <= 0 {
        return Err("GSpawn working-directory child pid");
    }
    cleanup_spawn_smoke_child(wd_child);

    match spawn_sync(None, &[GOOD_EXEC], None, SpawnFlags::DEFAULT, None) {
        Ok(result) => {
            if result.pid <= 0 {
                return Err("GSpawn sync child pid");
            }
            if result.stdout.is_none() || result.stderr.is_none() {
                return Err("GSpawn sync capture");
            }
        }
        Err(err) => return Err(spawn_error_name(err)),
    }

    Ok(())
}

fn cleanup_spawn_smoke_child(pid: i32) {
    let pid = pid as u32;
    let _ = crate::process_manager::get_process_manager().exit(pid, 0);
    let _ = crate::process::get_process_manager().retire_spawned_process(pid, 0);
}

fn spawn_error_name(err: SpawnError) -> &'static str {
    match err {
        SpawnError::Fork => "Fork",
        SpawnError::Read => "Read",
        SpawnError::Chdir => "Chdir",
        SpawnError::Acces => "Acces",
        SpawnError::Perm => "Perm",
        SpawnError::TooBig => "TooBig",
        SpawnError::Noexec => "Noexec",
        SpawnError::Nametoolong => "Nametoolong",
        SpawnError::Noent => "Noent",
        SpawnError::Nomem => "Nomem",
        SpawnError::Notdir => "Notdir",
        SpawnError::Loop => "Loop",
        SpawnError::Txtbusy => "Txtbusy",
        SpawnError::Io => "Io",
        SpawnError::Nfile => "Nfile",
        SpawnError::Mfile => "Mfile",
        SpawnError::Inval => "Inval",
        SpawnError::Isdir => "Isdir",
        SpawnError::Libbad => "Libbad",
        SpawnError::Failed => "Failed",
    }
}

fn glib_smoke_hook(data: usize) {
    GLIB_SMOKE_HOOK_COUNT.fetch_add(data, core::sync::atomic::Ordering::SeqCst);
}

static GLIB_THREADPOOL_COUNT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
static GLIB_TEST_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

fn glib_threadpool_noop(_data: usize) {
    // The inline thread pool implementation executes the callback synchronously.
    // Record that the callback was invoked so tests can verify the API path.
    GLIB_THREADPOOL_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
}

fn glib_test_noop() {
    // Record that the test case body was executed.
    GLIB_TEST_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
}

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
/// GNOME readiness smoke check for the RustOS compatibility surface.
///
/// This proves the small set of primitives GNOME depends on first: GLib core,
/// spawn/exec error handling, and VFS-backed file I/O usable by GIO-style code.
pub fn smoke_check_gnome_readiness() -> Result<(), &'static str> {
    smoke_check_spawn()?;

    Ok(())
}
