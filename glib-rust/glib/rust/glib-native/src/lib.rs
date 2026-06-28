//! Native Rust reimplementation of GLib.
//!
//! See [`docs/rust-migration.md`](../../docs/rust-migration.md) for the phased
//! migration plan.

// Dual-mode: `no_std` for the kernel; full `std` under `cargo test` or when
// building the host C static library (`c-abi` feature) so tests and C smoke
// links get a panic handler and global allocator.
#![cfg_attr(all(not(test), not(feature = "c-abi")), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;
#[cfg(all(not(test), not(target_os = "none")))]
extern crate std;

// Compatibility modules matching upstream GLib/GObject source splits.
pub mod gatomicarray;
pub mod gbase64;
pub mod gbinding;
pub mod gbindinggroup;
pub mod gclosure;
pub mod giowin32;
pub mod gmarshal;
pub mod gobject_query;
pub mod gobjectnotifyqueue;
pub mod gsignalgroup;
pub mod gsourceclosure;
pub mod gspawn_win32;
pub mod gspawn_win32_helper;
pub mod gthread_win32;
pub mod gtypemodule;
pub mod gtypeplugin;
pub mod gvaluetypes;
pub mod gwakeup;
pub mod gwin32;
pub mod win_iconv;

// When compiling for a no_std target, provide a delegating allocator and panic
// handler so the crate type-checks and links. The kernel binary calls
// `set_allocator` and `set_panic_handler` during early boot to register its
// own implementations; until then, alloc returns null and panic loops.
#[cfg(all(not(test), not(feature = "c-abi"), target_os = "none"))]
mod standalone_support {
    use alloc::alloc::{GlobalAlloc, Layout};
    use core::sync::atomic::{AtomicPtr, Ordering};

    static ALLOC_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
    static DEALLOC_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
    static REALLOC_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
    static ALLOC_ZEROED_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

    struct DelegatingAllocator;

    unsafe impl GlobalAlloc for DelegatingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let f = ALLOC_FN.load(Ordering::Relaxed);
            if f.is_null() {
                return core::ptr::null_mut();
            }
            // SAFETY: f was stored from a valid function pointer via set_allocator.
            let f: unsafe fn(Layout) -> *mut u8 = unsafe { core::mem::transmute(f) };
            unsafe { f(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            let f = DEALLOC_FN.load(Ordering::Relaxed);
            if f.is_null() {
                return;
            }
            // SAFETY: f was stored from a valid function pointer via set_allocator.
            let f: unsafe fn(*mut u8, Layout) = unsafe { core::mem::transmute(f) };
            unsafe { f(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let f = REALLOC_FN.load(Ordering::Relaxed);
            if f.is_null() {
                return core::ptr::null_mut();
            }
            // SAFETY: f was stored from a valid function pointer via set_allocator.
            let f: unsafe fn(*mut u8, Layout, usize) -> *mut u8 =
                unsafe { core::mem::transmute(f) };
            unsafe { f(ptr, layout, new_size) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let f = ALLOC_ZEROED_FN.load(Ordering::Relaxed);
            if f.is_null() {
                return core::ptr::null_mut();
            }
            // SAFETY: f was stored from a valid function pointer via set_allocator.
            let f: unsafe fn(Layout) -> *mut u8 = unsafe { core::mem::transmute(f) };
            unsafe { f(layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: DelegatingAllocator = DelegatingAllocator;

    /// Register the kernel's allocator functions.
    ///
    /// Must be called before any heap allocation.
    pub fn set_allocator(
        alloc: unsafe fn(Layout) -> *mut u8,
        dealloc: unsafe fn(*mut u8, Layout),
        realloc: unsafe fn(*mut u8, Layout, usize) -> *mut u8,
        alloc_zeroed: unsafe fn(Layout) -> *mut u8,
    ) {
        ALLOC_FN.store(alloc as *mut (), Ordering::Relaxed);
        DEALLOC_FN.store(dealloc as *mut (), Ordering::Relaxed);
        REALLOC_FN.store(realloc as *mut (), Ordering::Relaxed);
        ALLOC_ZEROED_FN.store(alloc_zeroed as *mut (), Ordering::Relaxed);
    }

    static PANIC_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let f = PANIC_FN.load(Ordering::Relaxed);
        if !f.is_null() {
            // SAFETY: f was stored from a valid function pointer via set_panic_handler.
            let f: fn(&core::panic::PanicInfo) -> ! = unsafe { core::mem::transmute(f) };
            f(info);
        }
        loop {}
    }

    /// Register the kernel's panic handler.
    pub fn set_panic_handler(handler: fn(&core::panic::PanicInfo) -> !) {
        PANIC_FN.store(handler as *mut (), Ordering::Relaxed);
    }
}

#[cfg(all(not(test), not(feature = "c-abi"), target_os = "none"))]
pub use standalone_support::{set_allocator, set_panic_handler};

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
pub mod checksum;
pub mod completion;
pub mod convert;
pub mod dataset;
pub mod date;
pub mod datetime;
pub mod dir;
pub mod endian;
pub mod environ;
pub mod error;
#[cfg(any(test, feature = "c-abi"))]
pub mod ffi;
#[cfg(test)]
pub mod ffi_parity;
pub mod fileutils;
pub mod gaction;
pub mod gactiongroup;
pub mod gactiongroupexporter;
pub mod gactionmap;
pub mod gappinfo;
pub mod gappinfomonitor;
pub mod gapplication;
pub mod gapplicationcommandline;
pub mod gapplicationimpl;
pub mod gasynchelper;
pub mod gasyncinitable;
pub mod gasyncresult;
pub mod gboxed;
pub mod gbufferedinputstream;
pub mod gbufferedoutputstream;
pub mod gbytesicon;
pub mod gcancellable;
pub mod gcharsetconverter;
pub mod gcontenttype;
pub mod gcontextspecificgroup;
pub mod gconverter;
pub mod gconverterinputstream;
pub mod gconverteroutputstream;
pub mod gcredentials;
pub mod gcredentialsmessage;
pub mod gdatagrambased;
pub mod gdatainputstream;
pub mod gdataoutputstream;
pub mod gdbusactiongroup;
pub mod gdbusaddress;
pub mod gdbusauth;
pub mod gdbusauthmechanism;
pub mod gdbusauthmechanismanon;
pub mod gdbusauthmechanismexternal;
pub mod gdbusauthmechanismsha1;
pub mod gdbusauthobserver;
pub mod gdbusconnection;
pub mod gdbusdaemon;
pub mod gdbuserror;
pub mod gdbusinterface;
pub mod gdbusinterfaceskeleton;
pub mod gdbusintrospection;
pub mod gdbusmenumodel;
pub mod gdbusmessage;
pub mod gdbusmethodinvocation;
pub mod gdbusnameowning;
pub mod gdbusnamewatching;
pub mod gdbusobject;
pub mod gdbusobjectmanager;
pub mod gdbusobjectmanagerclient;
pub mod gdbusobjectmanagerserver;
pub mod gdbusobjectproxy;
pub mod gdbusobjectskeleton;
pub mod gdbusproxy;
pub mod gdbusserver;
pub mod gdbusutils;
pub mod gdebugcontroller;
pub mod gdebugcontrollerdbus;
pub mod gdelayedsettingsbackend;
pub mod gdesktopappinfo;
pub mod gdocumentportal;
pub mod gdrive;
pub mod gdtlsclientconnection;
pub mod gdtlsconnection;
pub mod gdtlsserverconnection;
pub mod gdummyfile;
pub mod gdummyproxyresolver;
pub mod gdummytlsbackend;
pub mod gemblem;
pub mod gemblemedicon;
pub mod genums;
pub mod gfdonotificationbackend;
pub mod gfile;
pub mod gfileattribute;
pub mod gfiledescriptorbased;
pub mod gfileenumerator;
pub mod gfileicon;
pub mod gfileinfo;
pub mod gfileinputstream;
pub mod gfileiostream;
pub mod gfilemonitor;
pub mod gfilenamecompleter;
pub mod gfileoutputstream;
pub mod gfilterinputstream;
pub mod gfilteroutputstream;
pub mod ggtknotificationbackend;
pub mod ghttpproxy;
pub mod gicon;
pub mod ginetaddress;
pub mod ginetaddressmask;
pub mod ginetsocketaddress;
pub mod ginitable;
pub mod ginputstream;
pub mod gioenums;
pub mod gioerror;
pub mod giomodule;
pub mod gioscheduler;
pub mod giostream;
pub mod giotypes;
pub mod giptosmessage;
pub mod gipv6tclassmessage;
pub mod gkeyfilesettingsbackend;
pub mod glistmodel;
pub mod gliststore;
pub mod gloadableicon;
pub mod glocalfile;
pub mod glocalfileenumerator;
pub mod glocalfileinfo;
pub mod glocalfileinputstream;
pub mod glocalfileiostream;
pub mod glocalfilemonitor;
pub mod glocalfileoutputstream;
pub mod glocalvfs;
pub mod gmemoryinputstream;
pub mod gmemorymonitor;
pub mod gmemorymonitorbase;
pub mod gmemorymonitordbus;
pub mod gmemorymonitorpoll;
pub mod gmemorymonitorportal;
pub mod gmemorymonitorpsi;
pub mod gmemoryoutputstream;
pub mod gmemorysettingsbackend;
pub mod gmenu;
pub mod gmenuexporter;
pub mod gmenumodel;
pub mod gmodule;
pub mod gmount;
pub mod gmountoperation;
pub mod gnativesocketaddress;
pub mod gnativevolumemonitor;
pub mod gnetworkaddress;
pub mod gnetworking;
pub mod gnetworkmonitor;
pub mod gnetworkmonitorbase;
pub mod gnetworkmonitornetlink;
pub mod gnetworkmonitornm;
pub mod gnetworkmonitorportal;
pub mod gnetworkservice;
pub mod gnotification;
pub mod gnotificationbackend;
pub mod gnullsettingsbackend;
pub mod gobject;
pub mod gopenuriportal;
pub mod goutputstream;
pub mod gparam;
pub mod gparamspec;
pub mod gparamspecs;
pub mod gpermission;
pub mod gpollableinputstream;
pub mod gpollableoutputstream;
pub mod gpollableutils;
pub mod gpollfilemonitor;
pub mod gportalnotificationbackend;
pub mod gportalsupport;
pub mod gpowerprofilemonitor;
pub mod gpowerprofilemonitordbus;
pub mod gpowerprofilemonitorportal;
pub mod gpropertyaction;
pub mod gproxy;
pub mod gproxyaddress;
pub mod gproxyaddressenumerator;
pub mod gproxyresolver;
pub mod gproxyresolverportal;
pub mod gregistrysettingsbackend;
pub mod gremoteactiongroup;
pub mod gresolver;
pub mod gresource;
pub mod gresourcefile;
pub mod gsandbox;
pub mod gseekable;
pub mod gsettings;
pub mod gsettingsbackend;
pub mod gsettingsschema;
pub mod gsignal;
pub mod gsimpleaction;
pub mod gsimpleactiongroup;
pub mod gsimpleasyncresult;
pub mod gsimpleiostream;
pub mod gsimplepermission;
pub mod gsimpleproxyresolver;
pub mod gsocket;
pub mod gsocketaddress;
pub mod gsocketaddressenumerator;
pub mod gsocketclient;
pub mod gsocketconnectable;
pub mod gsocketconnection;
pub mod gsocketcontrolmessage;
pub mod gsocketinputstream;
pub mod gsocketlistener;
pub mod gsocketoutputstream;
pub mod gsocketservice;
pub mod gsocks4aproxy;
pub mod gsocks4proxy;
pub mod gsocks5proxy;
pub mod gsrvtarget;
pub mod gstring;
pub mod gsubprocess;
pub mod gsubprocesslauncher;
pub mod gtask;
pub mod gtcpconnection;
pub mod gtcpwrapperconnection;
pub mod gtestdbus;
pub mod gthemedicon;
pub mod gthreadedresolver;
pub mod gthreadedsocketservice;
pub mod gtlsbackend;
pub mod gtlscertificate;
pub mod gtlsclientconnection;
pub mod gtlsconnection;
pub mod gtlsdatabase;
pub mod gtlsfiledatabase;
pub mod gtlsinteraction;
pub mod gtlspassword;
pub mod gtlsserverconnection;
pub mod gtrashportal;
pub mod gtype;
pub mod gunionvolumemonitor;
pub mod gunixconnection;
pub mod gunixcredentialsmessage;
pub mod gunixfdlist;
pub mod gunixfdmessage;
pub mod gunixinputstream;
pub mod gunixmount;
pub mod gunixmounts;
pub mod gunixoutputstream;
pub mod gunixsocketaddress;
pub mod gunixvolume;
pub mod gunixvolumemonitor;
pub mod gvalue;
pub mod gvaluearray;
pub mod gvaluetransform;
pub mod gvfs;
pub mod gvolume;
pub mod gvolumemonitor;
pub mod gzlibcompressor;
pub mod gzlibdecompressor;
pub mod hash;
pub mod hmac;
pub mod hook;
pub mod hostutils;
pub mod iochannel;
pub mod keyfile;
pub mod list;
pub mod mainloop;
pub mod mappedfile;
pub mod markup;
pub mod mem;
pub mod messages;
pub mod node;
pub mod option;
pub mod pathbuf;
pub mod pattern;
pub mod poll;
pub mod primes;
pub mod printf;
pub mod ptr_array;
pub mod qsort;
pub mod quark;
pub mod queue;
pub mod rand;
pub mod rcbox;
pub mod refcount;
pub mod refstring;
pub mod regex;
pub mod relation;
pub mod scanner;
pub mod sequence;
pub mod shell;
pub mod slice;
pub mod spawn;
pub mod stdio;
pub mod strfuncs;
pub mod stringchunk;
pub mod strvbuilder;
pub mod testutils;
pub mod thread;
pub mod threadpool;
pub mod thumbnail_verify;
pub mod timer;
pub mod timezone;
pub mod trashstack;
pub mod tree;
pub mod tzif;
pub mod unicode;
pub mod unicode_norm;
pub mod uri;
pub mod utf8;
pub mod utils;
pub mod uuid;
pub mod variant;
pub mod varianttype;
pub mod version;

// Internal/private modules
pub mod gapplicationimpl_dbus;
pub mod gcontenttype_fdo;
pub mod gcontenttype_win32;
pub mod gdbusprivate;
pub mod giomodule_priv;
pub mod giounix_private;
pub mod giowin32_private;
pub mod gmarshal_internal;
pub mod gmemorymonitorwin32;
pub mod gsettings_mapping;
pub mod strinfo;

// Win32 platform modules
pub mod gwin32appinfo;
pub mod gwin32file_sync_stream;
pub mod gwin32inputstream;
pub mod gwin32mount;
pub mod gwin32networkmonitor;
pub mod gwin32notificationbackend;
pub mod gwin32outputstream;
pub mod gwin32packageparser;
pub mod gwin32registrykey;
pub mod gwin32sid;
pub mod gwin32volumemonitor;

// OSX platform module
pub mod gosxnetworkmonitor;

// CLI tool modules
pub mod gapplication_tool;
pub mod gdbus_tool;
pub mod gio_launch_desktop;
pub mod gio_querymodules;
pub mod gio_tool;
pub mod gio_tool_cat;
pub mod gio_tool_copy;
pub mod gio_tool_info;
pub mod gio_tool_launch;
pub mod gio_tool_list;
pub mod gio_tool_mime;
pub mod gio_tool_mkdir;
pub mod gio_tool_monitor;
pub mod gio_tool_mount;
pub mod gio_tool_move;
pub mod gio_tool_open;
pub mod gio_tool_remove;
pub mod gio_tool_rename;
pub mod gio_tool_save;
pub mod gio_tool_set;
pub mod gio_tool_trash;
pub mod gio_tool_tree;
pub mod glib_compile_resources;
pub mod glib_compile_schemas;
pub mod gresource_tool;
pub mod gsettings_tool;

// GObject Introspection
pub mod gdump;
pub mod gi_dump_types;
pub mod giarginfo;
pub mod gibaseinfo;
pub mod gibaseinfo_private;
pub mod gicallableinfo;
pub mod gicallbackinfo;
pub mod giconstantinfo;
pub mod gienuminfo;
pub mod gifieldinfo;
pub mod giflagsinfo;
pub mod gifunctioninfo;
pub mod giinterfaceinfo;
pub mod ginvoke;
pub mod giobjectinfo;
pub mod gipropertyinfo;
pub mod giregisteredtypeinfo;
pub mod girepository;
pub mod girepository_autocleanups;
pub mod girepository_private;
pub mod girffi;
pub mod girmodule;
pub mod girmodule_private;
pub mod girnode;
pub mod girnode_private;
pub mod giroffsets;
pub mod girparser;
pub mod girparser_private;
pub mod girwriter;
pub mod girwriter_private;
pub mod gisignalinfo;
pub mod gistructinfo;
pub mod gitypeinfo;
pub mod gitypelib;
pub mod gitypelib_internal;
pub mod gitypes;
pub mod giunioninfo;
pub mod giunresolvedinfo;
pub mod givalueinfo;
pub mod givfuncinfo;
pub mod gthash;

// Private header modules
pub mod gappinfoprivate;
pub mod gcontenttypeprivate;
pub mod gcredentialsprivate;
pub mod gdbusactiongroup_private;
pub mod gfileattribute_priv;
pub mod gfileinfo_priv;
pub mod gio_trace;
pub mod gioprivate;
pub mod giowin32_afunix;
pub mod giowin32_priv;
pub mod gmountprivate;
pub mod gnetworkingprivate;
pub mod gnotification_private;
pub mod gosxappinfo;
pub mod gsettingsbackendinternal;
pub mod gsettingsschema_internal;
pub mod gsubprocesslauncher_private;
pub mod gthreadedresolver_private;
pub mod gunixmounts_private;

// Win32 platform modules
pub mod gwin32filemonitor;
pub mod gwin32fsmonitorutils;
pub mod gwinhttpfile;
pub mod gwinhttpfileinputstream;
pub mod gwinhttpfileoutputstream;
pub mod gwinhttpvfs;
pub mod winhttp;

// XDG MIME modules
pub mod xdgmime;
pub mod xdgmimealias;
pub mod xdgmimecache;
pub mod xdgmimeglob;
pub mod xdgmimeicon;
pub mod xdgmimeint;
pub mod xdgmimemagic;
pub mod xdgmimeparent;

// inotify platform modules
pub mod ginotifyfilemonitor;
pub mod inotify_helper;
pub mod inotify_kernel;
pub mod inotify_missing;
pub mod inotify_path;
pub mod inotify_sub;

// kqueue platform modules
pub mod dep_list;
pub mod gkqueuefilemonitor;
pub mod kqueue_helper;
pub mod kqueue_missing;

// Umbrella modules
pub mod gio;
pub mod gio_autocleanups;

#[cfg(test)]
extern crate std;

pub use array::{ByteArray, GArray};
pub use asyncqueue::{async_queue_new, AsyncQueue};
pub use atomic::{AtomicInt, AtomicPointer, AtomicUInt};
pub use base64::{
    base64_decode, base64_decode_inplace, base64_encode, Base64Decoder, Base64Encoder,
};
pub use bitlock::{
    bit_lock, bit_trylock, bit_unlock, pointer_bit_lock, pointer_bit_trylock, pointer_bit_unlock,
};
pub use bytes::Bytes;
pub use cache::{Cache, CacheDestroyFunc, CacheDupFunc, CacheNewFunc};
pub use charset::{
    get_charset, get_codeset, get_console_charset, get_language_names, get_locale_variants,
};
pub use checked::{checked_add_size, checked_add_u32, checked_mul_size, checked_mul_u32};
pub use checksum::{
    checksum_type_get_length, compute_checksum_for_bytes, compute_checksum_for_data,
    compute_checksum_for_string, Checksum, ChecksumType,
};
pub use completion::{Completion, CompletionFunc, CompletionStrncmpFunc};
pub use convert::{
    convert_error_quark, filename_display_basename, filename_display_name, filename_from_uri,
    filename_to_uri, uri_list_extract_uris, ConvertError,
};
pub use dataset::{
    datalist_clear, datalist_foreach, datalist_id_get_data, datalist_id_remove_no_notify,
    datalist_id_set_data, datalist_id_set_data_full, datalist_init, DataList,
};
pub use date::{
    date_parse, get_days_in_month, is_leap_year, monday_weeks_in_year, sunday_weeks_in_year,
    valid_day, valid_dmy, valid_julian, valid_month, valid_weekday, valid_year, Date, DateDay,
    DateMonth, DateWeekday, DateYear, DATE_BAD_JULIAN,
};
pub use datetime::{
    DateTime, TimeSpan, TIME_SPAN_DAY, TIME_SPAN_HOUR, TIME_SPAN_MILLISECOND, TIME_SPAN_MINUTE,
    TIME_SPAN_SECOND,
};
pub use dir::{
    dir_open, register_dir_platform, Dir, DirError, DirPlatform, NoDirPlatform, DIR_CASE_SENSITIVE,
    DIR_NO_DOT_AND_DOTDOT,
};
pub use endian::{
    g_htonl, g_htons, g_ntohl, g_ntohs, swap_u16_le_be, swap_u32_le_be, swap_u64_le_be,
};
pub use environ::{
    environ_getenv, environ_setenv, environ_unsetenv, get_environ, getenv, listenv, setenv,
    unsetenv,
};
pub use error::{
    clear_error, error_copy, error_free, error_matches, error_new, error_new_literal, prefix_error,
    prefix_error_literal, propagate_error, propagate_prefixed_error, set_error, set_error_literal,
    steal_error, Error,
};
pub use fileutils::file_error_from_errno;
pub use fileutils::{
    build_filename, build_pathv, canonicalize_filename, file_error_quark, is_dir_separator,
    path_get_basename, path_get_dirname, path_is_absolute, path_skip_root, FileError, FileTest,
};
pub use gaction::{
    action_name_is_valid, action_parse_detailed_name, action_print_detailed_name, Action,
};
pub use gactiongroup::{ActionGroup, ActionInfo};
pub use gactiongroupexporter::ActionGroupExporter;
pub use gactionmap::{ActionEntry, ActionMap};
pub use gappinfo::{AppInfo, AppLaunchContext, SimpleAppInfo};
pub use gappinfomonitor::AppInfoMonitor;
pub use gapplication::{application_id_is_valid, Application, ApplicationFlags};
pub use gapplicationcommandline::ApplicationCommandLine;
pub use gapplicationimpl::ApplicationImpl;
pub use gasynchelper::AsyncHelper;
pub use gasyncinitable::AsyncInitable;
pub use gasyncresult::AsyncResult;
pub use gbufferedinputstream::BufferedInputStream;
pub use gbufferedoutputstream::BufferedOutputStream;
pub use gbytesicon::BytesIcon;
pub use gcancellable::{
    cancellable_get_current, cancellable_pop_current, cancellable_push_current,
    cancellable_source_new, GCancellable,
};
pub use gcharsetconverter::CharsetConverter;
pub use gcontenttype::{
    content_type_can_be_executable, content_type_equals, content_type_from_mime_type,
    content_type_get_description, content_type_get_mime_type, content_type_guess,
    content_type_is_a, content_type_is_mime_type, content_type_is_unknown,
    content_types_get_registered,
};
pub use gcontextspecificgroup::ContextSpecificGroup;
pub use gconverter::{Converter, ConverterFlags, ConverterResult};
pub use gconverterinputstream::ConverterInputStream;
pub use gconverteroutputstream::ConverterOutputStream;
pub use gcredentials::Credentials;
pub use gcredentialsmessage::CredentialsMessage;
pub use gdatagrambased::{Datagram, DatagramBased, IoCondition};
pub use gdatainputstream::{DataInputStream, DataStreamByteOrder, DataStreamNewlineType};
pub use gdataoutputstream::DataOutputStream;
pub use gdbusactiongroup::DBusActionGroup;
pub use gdbusaddress::{
    dbus_address_escape_value, dbus_address_get_for_bus_sync, get_session_bus_address,
    get_system_bus_address, is_address, is_supported_address, register_dbus_address_platform,
    DBusAddress, DBusAddressPlatform, NoDBusAddressPlatform,
};
pub use gdbusauth::{DBusAuth, DBusAuthRole, DBusAuthState};
pub use gdbusauthmechanism::{AuthMechanismState, DBusAuthMechanism, SimpleAuthMechanism};
pub use gdbusauthmechanismanon::DBusAuthMechanismAnon;
pub use gdbusauthmechanismexternal::DBusAuthMechanismExternal;
pub use gdbusauthmechanismsha1::DBusAuthMechanismSha1;
pub use gdbusauthobserver::DBusAuthObserver;
pub use gdbusconnection::{DBusConnection, DBusConnectionFlags};
pub use gdbusdaemon::DBusDaemon;
pub use gdbuserror::{
    dbus_error_encode_gerror, dbus_error_get_remote_error, dbus_error_is_remote_error,
    dbus_error_new_for_dbus_error, dbus_error_quark, dbus_error_register_error,
    dbus_error_register_error_domain, dbus_error_strip_remote_error, dbus_error_unregister_error,
    DBusError, DBusErrorEntry,
};
pub use gdbusinterface::{DBusInterface, SimpleDBusInterface};
pub use gdbusinterfaceskeleton::{DBusInterfaceSkeleton, DBusInterfaceSkeletonFlags};
pub use gdbusintrospection::{
    dbus_annotation_info_lookup, dbus_interface_info_lookup_method,
    dbus_interface_info_lookup_property, dbus_interface_info_lookup_signal,
    dbus_node_info_lookup_interface, DBusAnnotationInfo, DBusArgInfo, DBusInterfaceInfo,
    DBusMethodInfo, DBusNodeInfo, DBusPropertyInfo, DBusPropertyInfoFlags, DBusSignalInfo,
};
pub use gdbusmenumodel::{DBusMenuItem, DBusMenuModel};
pub use gdbusmessage::{
    DBusMessage, DBusMessageByteOrder, DBusMessageFlags, DBusMessageHeaderField, DBusMessageType,
};
pub use gdbusmethodinvocation::{DBusMethodInvocation, DBusReply};
pub use gdbusnameowning::{BusNameOwnerFlags, DBusNameOwning, NameOwnerState};
pub use gdbusnamewatching::{BusNameWatcherFlags, DBusNameWatching, NameWatchState};
pub use gdbusobject::{DBusObject, SimpleDBusObject};
pub use gdbusobjectmanager::DBusObjectManager;
pub use gdbusobjectmanagerclient::DBusObjectManagerClient;
pub use gdbusobjectmanagerserver::DBusObjectManagerServer;
pub use gdbusobjectproxy::DBusObjectProxy;
pub use gdbusobjectskeleton::DBusObjectSkeleton;
pub use gdbusproxy::{DBusBusType, DBusProxy, DBusProxyFlags};
pub use gdbusserver::{DBusServer, DBusServerFlags};
pub use gdbusutils::{
    escape_object_path, generate_guid, is_guid, is_interface_name, is_member_name, is_name,
    is_unique_name,
};
pub use gdebugcontroller::DebugController;
pub use gdebugcontrollerdbus::DebugControllerDBus;
pub use gdelayedsettingsbackend::DelayedSettingsBackend;
pub use gdesktopappinfo::DesktopAppInfo;
pub use gdocumentportal::DocumentPortal;
pub use gdrive::{Drive, DriveStartFlags, SimpleDrive};
pub use gdtlsclientconnection::DtlsClientConnection;
pub use gdtlsconnection::{DtlsConnection, RehandshakeMode};
pub use gdtlsserverconnection::DtlsServerConnection;
pub use gdummyfile::DummyFile;
pub use gdummyproxyresolver::DummyProxyResolver;
pub use gdummytlsbackend::DummyTlsBackend;
pub use gemblem::{Emblem, EmblemOrigin};
pub use gemblemedicon::EmblemedIcon;
pub use gfile::{
    register_file_platform, File, FileCreateFlags, FileInfo, FilePlatform, FileQueryInfoFlags,
    FileType, NoFilePlatform,
};
pub use gfileattribute::{
    FileAttributeInfo, FileAttributeInfoFlags, FileAttributeInfoList, FileAttributeType,
};
pub use gfiledescriptorbased::FileDescriptorBased;
pub use gfileenumerator::FileEnumerator;
pub use gfileicon::FileIcon;
pub use gfileinfo::{
    FileAttributeValue, FILE_ATTRIBUTE_ETAG_VALUE, FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
    FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME, FILE_ATTRIBUTE_STANDARD_IS_BACKUP,
    FILE_ATTRIBUTE_STANDARD_IS_HIDDEN, FILE_ATTRIBUTE_STANDARD_IS_SYMLINK,
    FILE_ATTRIBUTE_STANDARD_NAME, FILE_ATTRIBUTE_STANDARD_SIZE, FILE_ATTRIBUTE_STANDARD_SORT_ORDER,
    FILE_ATTRIBUTE_STANDARD_SYMLINK_TARGET, FILE_ATTRIBUTE_STANDARD_TYPE,
    FILE_ATTRIBUTE_TIME_MODIFIED,
};
pub use gfileinputstream::FileInputStream;
pub use gfileiostream::FileIOStream;
pub use gfilemonitor::{FileMonitor, FileMonitorEvent};
pub use gfilenamecompleter::FilenameCompleter;
pub use gfileoutputstream::FileOutputStream;
pub use gfilterinputstream::FilterInputStream;
pub use gfilteroutputstream::FilterOutputStream;
pub use ghttpproxy::HttpProxy as GHttpProxy;
pub use gicon::Icon;
pub use ginetaddress::{InetAddrBytes, InetAddress, SocketFamily};
pub use ginetaddressmask::{InetAddressMask, InetAddressMaskError};
pub use ginetsocketaddress::{InetSocketAddress, SockaddrIn, SockaddrIn6};
pub use ginitable::Initable;
pub use ginputstream::{InputStream, InputStreamImpl, MemoryInputStream};
pub use gioenums::{
    AppInfoCreateFlags, FileCopyFlags, FileMonitorEvent as GioFileMonitorEvent,
    FileQueryInfoFlags as GioFileQueryInfoFlags, MountUnmountFlags as GioMountUnmountFlags,
};
pub use gioerror::{
    io_error_from_errno, io_error_from_file_error, io_error_from_win32_error, io_error_quark,
    IOErrorEnum,
};
pub use giomodule::{
    io_module_build_path, io_module_close, io_module_open, io_module_registry_len,
    io_module_symbol, IoModule, IoModulePlatform, NoIoModulePlatform,
};
pub use gioscheduler::IoScheduler;
pub use giostream::IOStream;
pub use giptosmessage::IPTosMessage;
pub use gipv6tclassmessage::IPv6TClassMessage;
pub use glistmodel::{ItemType, ListModel};
pub use gliststore::ListStore;
pub use gloadableicon::LoadableIcon;
pub use glocalfile::LocalFile;
pub use glocalfileenumerator::LocalFileEnumerator;
pub use glocalfileinfo::{LocalFileInfo, LocalFileType};
pub use glocalfileinputstream::LocalFileInputStream;
pub use glocalfileiostream::LocalFileIOStream;
pub use glocalfilemonitor::LocalFileMonitor;
pub use glocalfileoutputstream::LocalFileOutputStream;
pub use glocalvfs::LocalVfs as GLocalVfs;
pub use gmemorymonitor::{memory_monitor_get_default, MemoryMonitor, MemoryPressureLevel};
pub use gmemorymonitorbase::{
    low_memory_level_to_pressure, memory_monitor_base_level_enum_to_byte,
    memory_monitor_base_query_mem_ratio, MemoryMonitorBase, MemoryMonitorLowMemoryLevel,
    MemoryMonitorWarningLevel, RECOVERY_INTERVAL_US,
};
pub use gmemorymonitordbus::MemoryMonitorDBus;
pub use gmemorymonitorpoll::MemoryMonitorPoll;
pub use gmemorymonitorportal::MemoryMonitorPortal;
pub use gmemorymonitorpsi::MemoryMonitorPsi;
pub use gmenu::{Menu, MenuItem};
pub use gmenuexporter::MenuExporter;
pub use gmenumodel::{MenuModel, SimpleMenuModel};
pub use gmodule::{
    module_build_path, module_close, module_error, module_error_quark, module_make_resident,
    module_name, module_open, module_open_full, module_supported, module_symbol,
    parse_libtool_archive, GModule, GModuleCheckInit, GModuleError, GModuleFlags, GModuleUnload,
    LibtoolArchive, ModuleHandle, ModulePlatform, NoModulePlatform,
};
#[cfg(test)]
pub use gmodule::{register_host_module_platform_for_tests, HostModulePlatform};
pub use gmount::{Mount, SimpleMount};
pub use gmountoperation::{AskPasswordFlags, MountOperation, MountOperationResult, PasswordSave};
pub use gnativesocketaddress::NativeSocketAddress;
pub use gnativevolumemonitor::NativeVolumeMonitor;
pub use gnetworkaddress::{NetworkAddress, NetworkAddressError};
pub use gnetworkmonitor::{NetworkConnectivity, NetworkMonitor};
pub use gnetworkmonitorbase::{NetworkConnectivity as BaseNetworkConnectivity, NetworkMonitorBase};
pub use gnetworkmonitornetlink::NetworkMonitorNetlink;
pub use gnetworkmonitornm::{NMConnectivity, NetworkMonitorNM};
pub use gnetworkmonitorportal::NetworkMonitorPortal;
pub use gnetworkservice::NetworkService;
pub use gnotification::{Notification, NotificationButton, NotificationPriority};
pub use gnotificationbackend::NotificationBackend;
pub use gobject::{
    object_new, object_new_with_params, GObject, ObjectFlags, PropertyBinding, WeakRefCallback,
};
pub use gopenuriportal::OpenURIPortal;
pub use goutputstream::{
    MemoryOutputStream, OutputStream, OutputStreamImpl, OutputStreamSpliceFlags,
};
pub use gparamspec::{
    find_property, find_property_by_id, install_properties, property_names, ParamFlags, ParamID,
    ParamSpec,
};
pub use gpermission::Permission;
pub use gpollableinputstream::PollableInputStream;
pub use gpollableoutputstream::{PollableOutputStream, PollableReturn};
pub use gpollableutils::{is_closed, is_readable, is_writable, PollableCondition};
pub use gpollfilemonitor::PollFileMonitor;
pub use gportalsupport::PortalSupport;
pub use gpowerprofilemonitor::{PowerProfile, PowerProfileMonitor};
pub use gpowerprofilemonitordbus::{PowerProfile as DBusPowerProfile, PowerProfileMonitorDBus};
pub use gpowerprofilemonitorportal::PowerProfileMonitorPortal;
pub use gpropertyaction::PropertyAction;
pub use gproxy::{
    get_default_for_protocol, register_proxy, DirectProxy, HttpProxy, Proxy, Socks5Proxy,
};
pub use gproxyaddress::ProxyAddress;
pub use gproxyaddressenumerator::{ProxyAddressEnumerator, ProxyUriLookup};
pub use gproxyresolver::ProxyResolver;
pub use gproxyresolverportal::ProxyResolverPortal;
pub use gregistrysettingsbackend::RegistrySettingsBackend;
pub use gremoteactiongroup::{RemoteAction, RemoteActionGroup};
pub use gresolver::{NoopResolver, Resolver, ResolverError};
pub use gresource::{
    resource_error_quark, resources_lookup_data, resources_register, Resource, ResourceError,
    ResourceLookupFlags,
};
pub use gresourcefile::ResourceFile;
pub use gsandbox::{Sandbox, SandboxType};
pub use gseekable::Seekable;
pub use gsettings::{Settings, SettingsValue};
pub use gsettingsbackend::SettingsBackend;
pub use gsettingsschema::{SettingsSchema, SettingsSchemaKey, SettingsSchemaSource};
pub use gsignal::{
    signal_connect, signal_connect_by_name, signal_emit, signal_emit_by_name, signal_handler_block,
    signal_handler_disconnect, signal_handler_is_connected, signal_handler_unblock,
    signal_handlers_disconnect_all, signal_list_ids, signal_lookup, signal_n_handlers, signal_name,
    signal_new, signal_query, ConnectFlags, HandlerID, SignalCallback, SignalFlags, SignalID,
    SignalQuery,
};
pub use gsimpleaction::SimpleAction;
pub use gsimpleactiongroup::SimpleActionGroup;
pub use gsimpleasyncresult::SimpleAsyncResult;
pub use gsimpleiostream::SimpleIOStream;
pub use gsimplepermission::SimplePermission;
pub use gsimpleproxyresolver::SimpleProxyResolver;
pub use gsocket::{MockSocket, Socket, SocketProtocol, SocketType};
pub use gsocketaddress::SocketAddress;
pub use gsocketclient::SocketClient;
pub use gsocketconnectable::{SimpleConnectable, SocketAddressEnumerator, SocketConnectable};
pub use gsocketconnection::SocketConnection;
pub use gsocketcontrolmessage::SocketControlMessage;
pub use gsocketinputstream::SocketInputStream;
pub use gsocketlistener::SocketListener;
pub use gsocketoutputstream::SocketOutputStream;
pub use gsocketservice::{IncomingConnection, SocketService};
pub use gsocks4aproxy::Socks4AProxy;
pub use gsocks4proxy::Socks4Proxy;
pub use gsocks5proxy::Socks5Proxy as GSocks5Proxy;
pub use gsrvtarget::{srv_target_list_sort, SrvTarget};
pub use gstring::GString;
pub use gsubprocess::{Subprocess, SubprocessFlags};
pub use gsubprocesslauncher::SubprocessLauncher;
pub use gtask::Task;
pub use gtcpconnection::TcpConnection;
pub use gtcpwrapperconnection::TcpWrapperConnection;
pub use gtestdbus::TestDBus;
pub use gthemedicon::ThemedIcon;
pub use gthreadedresolver::ThreadedResolver;
pub use gthreadedsocketservice::ThreadedSocketService;
pub use gtlsbackend::TlsBackend;
pub use gtlscertificate::{TlsCertificate, TlsCertificateFlags};
pub use gtlsclientconnection::TlsClientConnection;
pub use gtlsconnection::TlsConnection;
pub use gtlsdatabase::{TlsDatabase, TlsDatabaseLookupFlags, TlsDatabaseVerifyFlags};
pub use gtlsfiledatabase::TlsFileDatabase;
pub use gtlsinteraction::{TlsInteraction, TlsInteractionResult, TlsPassword};
pub use gtlspassword::{TlsPassword as GTlsPassword, TlsPasswordFlags};
pub use gtlsserverconnection::{ClientCertificateMode, TlsServerConnection};
pub use gtrashportal::TrashPortal;
pub use gtype::{
    g_type_make_fundamental, type_add_interface, type_children, type_class_size, type_depth,
    type_from_name, type_fundamental, type_fundamental_next, type_get_all,
    type_get_type_registration_serial, type_init, type_instance_size, type_interfaces, type_is_a,
    type_is_abstract, type_is_classed, type_is_final, type_is_instantiatable, type_name,
    type_parent, type_query, type_register_fundamental, type_register_static,
    type_register_static_simple, type_value_table, GType, GTypeFlags, GTypeFundamentalFlags,
    GTypeInfo, GTypeValueTable, GValueData, ParamFlags as GTypeParamFlags,
    ParamSpec as GTypeParamSpec, SignalDef, TypeClassData, TypeInstanceData, TypeQuery,
    G_TYPE_BOOLEAN, G_TYPE_BOXED, G_TYPE_CHAR, G_TYPE_DOUBLE, G_TYPE_ENUM, G_TYPE_FLAGS,
    G_TYPE_FLOAT, G_TYPE_FUNDAMENTAL_MAX, G_TYPE_FUNDAMENTAL_SHIFT, G_TYPE_INT, G_TYPE_INT64,
    G_TYPE_INTERFACE, G_TYPE_INVALID, G_TYPE_LONG, G_TYPE_NONE, G_TYPE_OBJECT, G_TYPE_PARAM,
    G_TYPE_POINTER, G_TYPE_STRING, G_TYPE_UCHAR, G_TYPE_UINT, G_TYPE_UINT64, G_TYPE_ULONG,
    G_TYPE_VARIANT,
};
pub use gunionvolumemonitor::UnionVolumeMonitor;
pub use gunixconnection::UnixConnection;
pub use gunixcredentialsmessage::UnixCredentialsMessage;
pub use gunixfdlist::UnixFDList;
pub use gunixfdmessage::UnixFDMessage;
pub use gunixinputstream::UnixInputStream;
pub use gunixmount::UnixMountEntry;
pub use gunixmounts::UnixMounts;
pub use gunixoutputstream::UnixOutputStream;
pub use gunixsocketaddress::{SockaddrUn, UnixSocketAddress, UnixSocketAddressType, UNIX_PATH_MAX};
pub use gunixvolume::UnixVolume;
pub use gunixvolumemonitor::UnixVolumeMonitor;
pub use gvalue::{
    default_value_table_for, value_new_boolean, value_new_boxed, value_new_char, value_new_double,
    value_new_enum, value_new_flags, value_new_float, value_new_int, value_new_int64,
    value_new_object, value_new_pointer, value_new_string, value_new_uint, value_new_uint64,
    GValue, TransformFunc,
};
pub use gvaluetransform::{
    init_builtin_transforms, value_register_transform_func, value_transform, value_type_compatible,
    value_type_transformable,
};
pub use gvfs::{LocalVfs, Vfs};
pub use gvolume::{MountUnmountFlags, SimpleVolume, Volume};
pub use gvolumemonitor::{DriveEntry, VolumeEntry, VolumeMonitor};
pub use gzlibcompressor::{ZlibCompressor, ZlibCompressorFormat};
pub use gzlibdecompressor::ZlibDecompressor;
pub use hash::{
    direct_equal, direct_hash, double_equal, double_hash, int64_equal, int64_hash, int_equal,
    int_hash, str_equal, str_hash, HashTable, HashTableIter,
};
pub use hmac::{compute_hmac_for_bytes, compute_hmac_for_data, compute_hmac_for_string, Hmac};
pub use hook::{
    hook_compare_ids, DestroyNotify, Hook, HookCallback, HookCheckFunc, HookCompareFunc,
    HookFindFunc, HookFunc, HookList, HOOK_FLAG_ACTIVE, HOOK_FLAG_IN_CALL, HOOK_FLAG_MASK,
};
pub use hostutils::{
    hostname_is_ascii_encoded, hostname_is_ip_address, hostname_is_non_ascii, hostname_to_ascii,
    hostname_to_unicode,
};
pub use iochannel::{io_channel_error_quark, IOChannelError, IOError, IOFlags, IOStatus, SeekType};
pub use keyfile::{key_file_error_quark, KeyFile, KeyFileError, KeyFileFlags};
pub use list::{CompareFn, GList, GSList, List, SList};
pub use mainloop::{
    default_context, idle_add, source_remove, timeout_add, MainContext, MainContextFlags, MainLoop,
    Source, SourceCallbackFuncs, SourceCheckFunc, SourceDispatchFunc, SourceFinalizeFunc,
    SourceFlags, SourceFunc, SourceFuncs, SourcePrepareFunc, SOURCE_CONTINUE, SOURCE_REMOVE,
};
pub use mappedfile::{
    mapped_file_new, mapped_file_new_from_fd, register_mapped_file_platform, MappedFile,
    MappedFileError, MappedFilePlatform, NoMappedFilePlatform,
};
pub use markup::{
    escape_text, markup_error_quark, Attribute, Element, MarkupError, MarkupNode, MarkupParseFlags,
    MarkupParser,
};
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
pub use node::{NTree, Node, TraverseFlags, TraverseType};
pub use option::{
    option_context_new, option_error_quark, option_group_new, OptionArg, OptionContext,
    OptionEntry, OptionError, OptionFlags, OptionGroup, OPTION_REMAINING,
};
pub use pathbuf::PathBuf;
pub use pattern::{pattern_match_simple, PatternSpec};
pub use poll::{
    g_poll, register_poll_platform, test_poll_clear_fds, test_poll_register_fd, IOCondition,
    NoPollPlatform, PollFD, PollFunc, PollPlatform, TestPollPlatform, TimerPollPlatform,
};
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "android")
))]
pub use poll::{register_host_poll_platform_for_tests, HostPollPlatform};
pub use primes::spaced_primes_closest;
pub use printf::{printf_format, sprintf, vsprintf, PrintfArg};
pub use ptr_array::{GPointer, PtrArray, PtrCompareFunc};
pub use qsort::{sort_array, sort_array_unstable};
pub use quark::{
    intern_static_string, intern_string, quark_from_static_string, quark_from_string,
    quark_to_string, quark_try_string, Quark,
};
pub use queue::GQueue;
pub use rand::{
    random_boolean, random_double, random_double_range, random_int, random_int_range,
    random_set_seed, Rand,
};
pub use rcbox::{atomic_rc_box_alloc, atomic_rc_box_alloc0, rc_box_alloc0, AtomicRcBox, RcBox};
pub use refcount::{AtomicRefCount, RefCount};
pub use refstring::RefString;
pub use regex::{
    regex_error_quark, MatchInfo, Regex, RegexCompileFlags, RegexError, RegexMatchFlags,
};
pub use relation::{Relation, Tuple, Tuples};
pub use scanner::{
    CSET_a_2_z, ErrorType, Scanner, ScannerConfig, TokenType, TokenValue, CSET_A_2_Z, CSET_DIGITS,
};
pub use sequence::{Sequence, SequenceIter};
pub use shell::{shell_error_quark, shell_parse_argv, shell_quote, shell_unquote, ShellError};
pub use slice::{
    slice_alloc, slice_alloc0, slice_copy, slice_free1, slice_get_config, slice_set_config,
    SliceConfig,
};
pub use spawn::{
    register_spawn_platform, spawn_async, spawn_check_exit_status, spawn_check_wait_status,
    spawn_error_quark, spawn_exit_error_quark, spawn_sync, NoSpawnPlatform, Pid,
    SpawnChildSetupFunc, SpawnError, SpawnFlags, SpawnPlatform, SpawnResult,
};
pub use stdio::{
    access, mkdir, register_stdio_platform, stat, NoStdioPlatform, OpenFlags, StatBuf,
    StdioPlatform, F_OK, R_OK, S_IRGRP, S_IROTH, S_IRUSR, S_IRWXG, S_IRWXO, S_IRWXU, S_IWGRP,
    S_IWOTH, S_IWUSR, S_IXGRP, S_IXOTH, S_IXUSR, W_OK, X_OK,
};
pub use strfuncs::{
    ascii_digit_value, ascii_isalnum, ascii_isalpha, ascii_iscntrl, ascii_isdigit, ascii_isgraph,
    ascii_islower, ascii_isprint, ascii_ispunct, ascii_isspace, ascii_isupper, ascii_isxdigit,
    ascii_strcasecmp, ascii_strdown, ascii_strncasecmp, ascii_strtoll, ascii_strtoull, ascii_strup,
    ascii_tolower, ascii_toupper, ascii_xdigit_value, str_has_prefix, str_has_suffix, str_is_ascii,
    strcanon, strcasecmp, strchomp, strchug, strcmp, strcompress, strconcat, strdelimit, strdup,
    strdupv, strescape, strjoin, strjoinv, strlen, strndup, strndup_str, strnfill, strreverse,
    strrstr, strsplit, strsplit_set, strstr_len, strstrip, strv_contains, strv_equal, strv_length,
};
pub use stringchunk::StringChunk;
pub use strvbuilder::StrvBuilder;
pub use testutils::{
    assert_cmpint, assert_cmpstr, assert_false, assert_nonnull, assert_null, assert_true,
    test_add_data_func, test_add_func, test_assert_expected_messages, test_create_suite,
    test_expect_message, test_get_root, test_init, test_run, test_trap_subprocess, TestCase,
    TestSubprocessFlags, TestSuite, TestTrapFlags, TestTrapStatus,
};
#[cfg(test)]
pub use thread::HostThreadPlatform;
pub use thread::{
    register_thread_platform, thread_error_quark, GCond, GMutex, GRWLock, GRecMutex, GThread,
    NoThreadPlatform, Once, OnceStatus, ThreadError, ThreadHandle, ThreadPlatform,
};
pub use threadpool::{
    get_max_idle_time, get_max_unused_threads, get_num_unused_threads, set_max_idle_time,
    set_max_unused_threads, stop_unused_threads, ThreadPool, ThreadPoolError,
};
pub use thumbnail_verify::{
    get_thumbnail_path, is_thumbnail_path, thumbnail_verify, ThumbnailVerifyResult,
};
pub use timer::{monotonic_time_us, set_clock as timer_set_clock, ClockFn, Timer};
pub use timezone::{TimeType, TimeZone, TimeZoneError};
pub use trashstack::TrashStack;
pub use tree::{CompareDataFn, GTreeNode, TraverseFn, TraverseNodeFn, Tree};
pub use tzif::{TzifData, TzifError, TzifType};
pub use unicode::{
    combining_class as unicode_combining_class, normalize_nfd, unichar_canonical_decomposition,
    unichar_normalize, NormalizeMode, UnicodeBreakType, UnicodeNormalizeMode, UnicodeScript,
    UnicodeType,
};
pub use uri::{
    escape_string, is_valid, join, peek_scheme, unescape_string, Uri, UriError, UriFlags,
    UriHideFlags,
};
pub use utf8::{
    unichar_digit_value, unichar_isalnum, unichar_isalpha, unichar_iscntrl, unichar_isdigit,
    unichar_islower, unichar_isprint, unichar_ispunct, unichar_isspace, unichar_isupper,
    unichar_isxdigit, unichar_to_utf8, unichar_to_utf8_len, unichar_to_utf8_string,
    unichar_tolower, unichar_toupper, unichar_validate, unichar_xdigit_value, utf8_get_char,
    utf8_len, utf8_next_char, utf8_offset_to_pointer, utf8_pointer_to_offset, utf8_prev_char,
    utf8_strlen, utf8_validate, Unichar, Unichar2,
};
pub use utils::{
    get_application_name, get_prgname, set_application_name, set_prgname, NSEC_PER_SEC,
    OS_INFO_KEY_BUG_REPORT_URL, OS_INFO_KEY_DOCUMENTATION_URL, OS_INFO_KEY_HOME_URL,
    OS_INFO_KEY_ID, OS_INFO_KEY_NAME, OS_INFO_KEY_PRETTY_NAME, OS_INFO_KEY_PRIVACY_POLICY_URL,
    OS_INFO_KEY_SUPPORT_URL, OS_INFO_KEY_VERSION, OS_INFO_KEY_VERSION_CODENAME,
    OS_INFO_KEY_VERSION_ID, USEC_PER_SEC,
};
pub use uuid::{uuid_string_is_valid, uuid_string_random};
pub use variant::{
    parse as variant_parse, variant_parse_error_quark, Variant, VariantBuilder, VariantParseError,
};
pub use varianttype::{
    scan_type_string, type_equal, type_hash, type_string_is_valid, VariantClass, VariantType,
    VARIANT_TYPE_ANY, VARIANT_TYPE_ARRAY, VARIANT_TYPE_BASIC, VARIANT_TYPE_BOOLEAN,
    VARIANT_TYPE_BYTE, VARIANT_TYPE_BYTESTRING, VARIANT_TYPE_BYTESTRING_ARRAY,
    VARIANT_TYPE_DICTIONARY, VARIANT_TYPE_DICT_ENTRY, VARIANT_TYPE_DOUBLE, VARIANT_TYPE_HANDLE,
    VARIANT_TYPE_INT16, VARIANT_TYPE_INT32, VARIANT_TYPE_INT64, VARIANT_TYPE_MAYBE,
    VARIANT_TYPE_OBJECT_PATH, VARIANT_TYPE_SIGNATURE, VARIANT_TYPE_STRING,
    VARIANT_TYPE_STRING_ARRAY, VARIANT_TYPE_TUPLE, VARIANT_TYPE_UINT16, VARIANT_TYPE_UINT32,
    VARIANT_TYPE_UINT64, VARIANT_TYPE_UNIT, VARIANT_TYPE_VARDICT, VARIANT_TYPE_VARIANT,
};
pub use version::{
    check_version, check_version_bool, GLIB_BINARY_AGE, GLIB_INTERFACE_AGE, GLIB_MAJOR_VERSION,
    GLIB_MICRO_VERSION, GLIB_MINOR_VERSION,
};

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
