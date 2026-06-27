//! GIO error codes matching `gio/gioerror.h` / `gio/gioerror.c`.
//!
//! Provides:
//! - `IOErrorEnum` enum (50+ GIO error codes matching `GIOErrorEnum`).
//! - `io_error_quark()` — the `G_IO_ERROR` quark.
//! - `io_error_from_errno()` — errno → `IOErrorEnum` mapping.
//! - `io_error_from_file_error()` — `FileError` → `IOErrorEnum` mapping.
//! - `io_error_from_win32_error()` — Win32/WSA error code → `IOErrorEnum` mapping.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::fileutils::{file_error_from_errno, FileError};
use crate::quark::{quark_from_static_string, Quark};

// ─────────────────────────── GIOErrorEnum ─────────────────────────────────

/// GIO error codes (`GIOErrorEnum`).
///
/// Matches the upstream enum order so discriminant values are stable
/// across the C and Rust implementations. `ConnectionClosed` is an
/// alias for `BrokenPipe` (same value), matching upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum IOErrorEnum {
    /// Generic failure (`G_IO_ERROR_FAILED`).
    Failed = 0,
    /// File not found (`G_IO_ERROR_NOT_FOUND`).
    NotFound = 1,
    /// File already exists (`G_IO_ERROR_EXISTS`).
    Exists = 2,
    /// File is a directory (`G_IO_ERROR_IS_DIRECTORY`).
    IsDirectory = 3,
    /// File is not a directory (`G_IO_ERROR_NOT_DIRECTORY`).
    NotDirectory = 4,
    /// Directory is not empty (`G_IO_ERROR_NOT_EMPTY`).
    NotEmpty = 5,
    /// File is not a regular file (`G_IO_ERROR_NOT_REGULAR_FILE`).
    NotRegularFile = 6,
    /// File is not a symbolic link (`G_IO_ERROR_NOT_SYMBOLIC_LINK`).
    NotSymbolicLink = 7,
    /// File cannot be mounted (`G_IO_ERROR_NOT_MOUNTABLE_FILE`).
    NotMountableFile = 8,
    /// Filename too long (`G_IO_ERROR_FILENAME_TOO_LONG`).
    FilenameTooLong = 9,
    /// Invalid filename (`G_IO_ERROR_INVALID_FILENAME`).
    InvalidFilename = 10,
    /// Too many symbolic links (`G_IO_ERROR_TOO_MANY_LINKS`).
    TooManyLinks = 11,
    /// No space left (`G_IO_ERROR_NO_SPACE`).
    NoSpace = 12,
    /// Invalid argument (`G_IO_ERROR_INVALID_ARGUMENT`).
    InvalidArgument = 13,
    /// Permission denied (`G_IO_ERROR_PERMISSION_DENIED`).
    PermissionDenied = 14,
    /// Operation not supported (`G_IO_ERROR_NOT_SUPPORTED`).
    NotSupported = 15,
    /// File isn't mounted (`G_IO_ERROR_NOT_MOUNTED`).
    NotMounted = 16,
    /// File is already mounted (`G_IO_ERROR_ALREADY_MOUNTED`).
    AlreadyMounted = 17,
    /// File was closed (`G_IO_ERROR_CLOSED`).
    Closed = 18,
    /// Operation was cancelled (`G_IO_ERROR_CANCELLED`).
    Cancelled = 19,
    /// Operations are still pending (`G_IO_ERROR_PENDING`).
    Pending = 20,
    /// File is read-only (`G_IO_ERROR_READ_ONLY`).
    ReadOnly = 21,
    /// Backup couldn't be created (`G_IO_ERROR_CANT_CREATE_BACKUP`).
    CantCreateBackup = 22,
    /// Entity tag was incorrect (`G_IO_ERROR_WRONG_ETAG`).
    WrongEtag = 23,
    /// Operation timed out (`G_IO_ERROR_TIMED_OUT`).
    TimedOut = 24,
    /// Operation would recurse (`G_IO_ERROR_WOULD_RECURSE`).
    WouldRecurse = 25,
    /// File is busy (`G_IO_ERROR_BUSY`).
    Busy = 26,
    /// Operation would block (`G_IO_ERROR_WOULD_BLOCK`).
    WouldBlock = 27,
    /// Host not found (`G_IO_ERROR_HOST_NOT_FOUND`).
    HostNotFound = 28,
    /// Operation would merge files (`G_IO_ERROR_WOULD_MERGE`).
    WouldMerge = 29,
    /// Operation failed and was already handled (`G_IO_ERROR_FAILED_HANDLED`).
    FailedHandled = 30,
    /// Too many open files (`G_IO_ERROR_TOO_MANY_OPEN_FILES`).
    TooManyOpenFiles = 31,
    /// Object not initialized (`G_IO_ERROR_NOT_INITIALIZED`).
    NotInitialized = 32,
    /// Address already in use (`G_IO_ERROR_ADDRESS_IN_USE`).
    AddressInUse = 33,
    /// Need more input (`G_IO_ERROR_PARTIAL_INPUT`).
    PartialInput = 34,
    /// Invalid data (`G_IO_ERROR_INVALID_DATA`).
    InvalidData = 35,
    /// D-Bus error (`G_IO_ERROR_DBUS_ERROR`).
    DbusError = 36,
    /// Host unreachable (`G_IO_ERROR_HOST_UNREACHABLE`).
    HostUnreachable = 37,
    /// Network unreachable (`G_IO_ERROR_NETWORK_UNREACHABLE`).
    NetworkUnreachable = 38,
    /// Connection refused (`G_IO_ERROR_CONNECTION_REFUSED`).
    ConnectionRefused = 39,
    /// Proxy failed (`G_IO_ERROR_PROXY_FAILED`).
    ProxyFailed = 40,
    /// Proxy auth failed (`G_IO_ERROR_PROXY_AUTH_FAILED`).
    ProxyAuthFailed = 41,
    /// Proxy needs auth (`G_IO_ERROR_PROXY_NEED_AUTH`).
    ProxyNeedAuth = 42,
    /// Proxy not allowed (`G_IO_ERROR_PROXY_NOT_ALLOWED`).
    ProxyNotAllowed = 43,
    /// Broken pipe (`G_IO_ERROR_BROKEN_PIPE`).
    BrokenPipe = 44,
    /// Not connected (`G_IO_ERROR_NOT_CONNECTED`).
    NotConnected = 45,
    /// Message too large (`G_IO_ERROR_MESSAGE_TOO_LARGE`).
    MessageTooLarge = 46,
    /// No such device (`G_IO_ERROR_NO_SUCH_DEVICE`).
    NoSuchDevice = 47,
    /// Destination unset (`G_IO_ERROR_DESTINATION_UNSET`).
    DestinationUnset = 48,
}

impl IOErrorEnum {
    /// Numeric error code matching the upstream enum discriminant.
    pub fn to_code(self) -> i32 {
        self as i32
    }

    /// Alias for `BrokenPipe`, matching upstream
    /// `G_IO_ERROR_CONNECTION_CLOSED = G_IO_ERROR_BROKEN_PIPE`.
    pub const CONNECTION_CLOSED: IOErrorEnum = IOErrorEnum::BrokenPipe;
}

// ──────────────────────────── quark ───────────────────────────────────────

/// Quark for the GIO error domain (`g_io_error_quark`).
pub fn io_error_quark() -> Quark {
    quark_from_static_string(Some("g-io-error-quark"))
}

// ─────────────────────── from_file_error ──────────────────────────────────

/// Convert a `FileError` into an `IOErrorEnum`
/// (`g_io_error_from_file_error`).
///
/// Mirrors the upstream `switch`. `BadF`, `Failed`, `Fault`, `Intr`,
/// `Io` all map to `Failed`; `NoSpc` and `NoMem` both map to `NoSpace`;
/// `MFile` and `NFile` both map to `TooManyOpenFiles`.
pub fn io_error_from_file_error(file_error: FileError) -> IOErrorEnum {
    match file_error {
        FileError::Exist => IOErrorEnum::Exists,
        FileError::IsDir => IOErrorEnum::IsDirectory,
        FileError::Acces => IOErrorEnum::PermissionDenied,
        FileError::NameTooLong => IOErrorEnum::FilenameTooLong,
        FileError::NoEnt => IOErrorEnum::NotFound,
        FileError::NotDir => IOErrorEnum::NotDirectory,
        FileError::Nxio => IOErrorEnum::NotRegularFile,
        FileError::NoDev => IOErrorEnum::NoSuchDevice,
        FileError::RoFs => IOErrorEnum::ReadOnly,
        FileError::TxtBsy => IOErrorEnum::Busy,
        FileError::Loop => IOErrorEnum::TooManyLinks,
        FileError::NoSpc | FileError::NoMem => IOErrorEnum::NoSpace,
        FileError::MFile | FileError::NFile => IOErrorEnum::TooManyOpenFiles,
        FileError::Inval => IOErrorEnum::InvalidArgument,
        FileError::Pipe => IOErrorEnum::BrokenPipe,
        FileError::Again => IOErrorEnum::WouldBlock,
        FileError::Perm => IOErrorEnum::PermissionDenied,
        FileError::NoSys => IOErrorEnum::NotSupported,
        // BadF, Failed, Fault, Intr, Io all map to Failed.
        FileError::BadF
        | FileError::Failed
        | FileError::Fault
        | FileError::Intr
        | FileError::Io => IOErrorEnum::Failed,
    }
}

// ───────────────────────── from_errno ─────────────────────────────────────

/// Convert an `errno` value into an `IOErrorEnum`
/// (`g_io_error_from_errno`).
///
/// First delegates to `file_error_from_errno` + `io_error_from_file_error`
/// (matching upstream's two-step conversion), then handles additional
/// errno codes that don't have a `FileError` counterpart. Unknown
/// errnos return `IOErrorEnum::Failed`.
pub fn io_error_from_errno(err_no: i32) -> IOErrorEnum {
    let file_error = file_error_from_errno(err_no);
    let io_error = io_error_from_file_error(file_error);
    if io_error != IOErrorEnum::Failed {
        return io_error;
    }

    // Errno values from <errno.h> (Linux/glibc).
    const EMLINK: i32 = 31;
    const ENOMSG: i32 = 42;
    const ENODATA: i32 = 61;
    const EBADMSG: i32 = 74;
    const ECANCELED: i32 = 125;
    const ENOTEMPTY: i32 = 39;
    const ENOTSUP: i32 = 95;
    const EOPNOTSUPP: i32 = 95;
    const EPROTONOSUPPORT: i32 = 93;
    const ESOCKTNOSUPPORT: i32 = 94;
    const EPFNOSUPPORT: i32 = 96;
    const EAFNOSUPPORT: i32 = 97;
    const ETIMEDOUT: i32 = 110;
    const EBUSY: i32 = 16;
    const EWOULDBLOCK: i32 = 11;
    const EAGAIN: i32 = 11;
    const EADDRINUSE: i32 = 98;
    const EHOSTUNREACH: i32 = 113;
    const ENETUNREACH: i32 = 101;
    const ENETDOWN: i32 = 100;
    const ECONNREFUSED: i32 = 111;
    const EADDRNOTAVAIL: i32 = 99;
    const ECONNRESET: i32 = 104;
    const ENOTCONN: i32 = 107;
    const EDESTADDRREQ: i32 = 89;
    const EMSGSIZE: i32 = 90;
    const ENOTSOCK: i32 = 88;

    match err_no {
        EMLINK => IOErrorEnum::TooManyLinks,
        ENOMSG | ENODATA | EBADMSG => IOErrorEnum::InvalidData,
        ECANCELED => IOErrorEnum::Cancelled,
        ENOTEMPTY => IOErrorEnum::NotEmpty,
        ENOTSUP | EOPNOTSUPP | EPROTONOSUPPORT | ESOCKTNOSUPPORT | EPFNOSUPPORT | EAFNOSUPPORT => {
            IOErrorEnum::NotSupported
        }
        ETIMEDOUT => IOErrorEnum::TimedOut,
        EBUSY => IOErrorEnum::Busy,
        EWOULDBLOCK | EAGAIN => IOErrorEnum::WouldBlock,
        EADDRINUSE => IOErrorEnum::AddressInUse,
        EHOSTUNREACH | ENETDOWN => IOErrorEnum::HostUnreachable,
        ENETUNREACH => IOErrorEnum::NetworkUnreachable,
        ECONNREFUSED | EADDRNOTAVAIL => IOErrorEnum::ConnectionRefused,
        ECONNRESET => IOErrorEnum::CONNECTION_CLOSED,
        ENOTCONN => IOErrorEnum::NotConnected,
        EDESTADDRREQ => IOErrorEnum::DestinationUnset,
        EMSGSIZE => IOErrorEnum::MessageTooLarge,
        ENOTSOCK => IOErrorEnum::InvalidArgument,
        _ => IOErrorEnum::Failed,
    }
}

// ─────────────────────── from_win32_error ─────────────────────────────────

/// Common Win32 error codes (`winerror.h`).
pub mod win32_error {
    pub const ERROR_FILE_NOT_FOUND: u32 = 2;
    pub const ERROR_PATH_NOT_FOUND: u32 = 3;
    pub const ERROR_ACCESS_DENIED: u32 = 5;
    pub const ERROR_INVALID_HANDLE: u32 = 6;
    pub const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
    pub const ERROR_INVALID_PARAMETER: u32 = 87;
    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    pub const ERROR_PIPE_NOT_CONNECTED: u32 = 233;
    pub const ERROR_PIPE_LISTENING: u32 = 536;
    pub const ERROR_OPERATION_ABORTED: u32 = 995;
    pub const WSA_INVALID_HANDLE: u32 = 6;
    pub const WSA_INVALID_PARAMETER: u32 = 87;
    pub const WSAEINTR: u32 = 10004;
    pub const WSAEBADF: u32 = 10009;
    pub const WSAEACCES: u32 = 10013;
    pub const WSAEFAULT: u32 = 10014;
    pub const WSAEINVAL: u32 = 10022;
    pub const WSAEMFILE: u32 = 10024;
    pub const WSAEWOULDBLOCK: u32 = 10035;
    pub const WSAEINPROGRESS: u32 = 10036;
    pub const WSAEALREADY: u32 = 10037;
    pub const WSAENOTSOCK: u32 = 10038;
    pub const WSAEDESTADDRREQ: u32 = 10039;
    pub const WSAEMSGSIZE: u32 = 10040;
    pub const WSAEPROTOTYPE: u32 = 10041;
    pub const WSAENOPROTOOPT: u32 = 10042;
    pub const WSAEPROTONOSUPPORT: u32 = 10043;
    pub const WSAESOCKTNOSUPPORT: u32 = 10044;
    pub const WSAEOPNOTSUPP: u32 = 10045;
    pub const WSAEPFNOSUPPORT: u32 = 10046;
    pub const WSAEAFNOSUPPORT: u32 = 10047;
    pub const WSAEADDRINUSE: u32 = 10048;
    pub const WSAEADDRNOTAVAIL: u32 = 10049;
    pub const WSAENETDOWN: u32 = 10050;
    pub const WSAENETUNREACH: u32 = 10051;
    pub const WSAENETRESET: u32 = 10052;
    pub const WSAECONNABORTED: u32 = 10053;
    pub const WSAECONNRESET: u32 = 10054;
    pub const WSAENOBUFS: u32 = 10055;
    pub const WSAEISCONN: u32 = 10056;
    pub const WSAENOTCONN: u32 = 10057;
    pub const WSAESHUTDOWN: u32 = 10058;
    pub const WSAETOOMANYREFS: u32 = 10059;
    pub const WSAETIMEDOUT: u32 = 10060;
    pub const WSAECONNREFUSED: u32 = 10061;
    pub const WSAELOOP: u32 = 10062;
    pub const WSAENAMETOOLONG: u32 = 10063;
    pub const WSAEHOSTDOWN: u32 = 10064;
    pub const WSAEHOSTUNREACH: u32 = 10065;
    pub const WSAENOTEMPTY: u32 = 10066;
    pub const WSAECANCELLED: u32 = 10103;
}

/// Convert a Win32 or Winsock error code into an `IOErrorEnum`
/// (`g_io_error_from_win32_error`).
///
/// Maps the same codes handled by upstream GLib on Windows. Unknown codes
/// return `IOErrorEnum::Failed`. The mapping table is platform-independent
/// so callers can translate stored Win32 codes on any host.
pub fn io_error_from_win32_error(error_code: u32) -> IOErrorEnum {
    use win32_error::*;

    match error_code {
        WSAEADDRINUSE => IOErrorEnum::AddressInUse,
        WSAEWOULDBLOCK | WSAEINPROGRESS | WSAEALREADY => IOErrorEnum::WouldBlock,

        WSAEACCES | ERROR_ACCESS_DENIED => IOErrorEnum::PermissionDenied,

        WSA_INVALID_PARAMETER
        | WSAEINVAL
        | WSAEBADF
        | WSAENOTSOCK
        | ERROR_INVALID_HANDLE
        | ERROR_INVALID_PARAMETER => IOErrorEnum::InvalidArgument,

        WSAEPROTONOSUPPORT => IOErrorEnum::NotSupported,

        WSAECANCELLED | ERROR_OPERATION_ABORTED => IOErrorEnum::Cancelled,

        WSAESOCKTNOSUPPORT | WSAEOPNOTSUPP | WSAEPFNOSUPPORT | WSAEAFNOSUPPORT => {
            IOErrorEnum::NotSupported
        }

        WSAECONNRESET | WSAENETRESET | WSAESHUTDOWN | ERROR_PIPE_NOT_CONNECTED => {
            IOErrorEnum::CONNECTION_CLOSED
        }

        WSAEHOSTUNREACH | WSAEHOSTDOWN => IOErrorEnum::HostUnreachable,

        WSAENETUNREACH | WSAENETDOWN => IOErrorEnum::NetworkUnreachable,

        WSAECONNREFUSED | WSAEADDRNOTAVAIL => IOErrorEnum::ConnectionRefused,

        WSAETIMEDOUT => IOErrorEnum::TimedOut,

        WSAENOTCONN | ERROR_PIPE_LISTENING => IOErrorEnum::NotConnected,

        WSAEMSGSIZE => IOErrorEnum::MessageTooLarge,

        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => IOErrorEnum::NotFound,

        ERROR_ALREADY_EXISTS => IOErrorEnum::Exists,

        WSAENOTEMPTY => IOErrorEnum::NotEmpty,

        WSAENAMETOOLONG => IOErrorEnum::FilenameTooLong,

        WSAELOOP => IOErrorEnum::TooManyLinks,

        WSAENOBUFS | ERROR_NOT_ENOUGH_MEMORY => IOErrorEnum::NoSpace,

        WSAEMFILE => IOErrorEnum::TooManyOpenFiles,

        WSAEDESTADDRREQ => IOErrorEnum::DestinationUnset,

        WSAEFAULT | WSAEINTR => IOErrorEnum::Failed,

        _ => IOErrorEnum::Failed,
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_values_match_upstream() {
        assert_eq!(IOErrorEnum::Failed as i32, 0);
        assert_eq!(IOErrorEnum::NotFound as i32, 1);
        assert_eq!(IOErrorEnum::BrokenPipe as i32, 44);
        assert_eq!(IOErrorEnum::CONNECTION_CLOSED as i32, 44);
        assert_eq!(IOErrorEnum::NoSuchDevice as i32, 47);
        assert_eq!(IOErrorEnum::DestinationUnset as i32, 48);
    }

    #[test]
    fn connection_closed_is_alias_for_broken_pipe() {
        assert_eq!(IOErrorEnum::CONNECTION_CLOSED, IOErrorEnum::BrokenPipe);
    }

    #[test]
    fn to_code() {
        assert_eq!(IOErrorEnum::Failed.to_code(), 0);
        assert_eq!(IOErrorEnum::TimedOut.to_code(), 24);
        assert_eq!(IOErrorEnum::DbusError.to_code(), 36);
    }

    #[test]
    fn quark_is_nonzero() {
        assert!(io_error_quark() > 0);
    }

    #[test]
    fn from_file_error_mappings() {
        assert_eq!(
            io_error_from_file_error(FileError::Exist),
            IOErrorEnum::Exists
        );
        assert_eq!(
            io_error_from_file_error(FileError::IsDir),
            IOErrorEnum::IsDirectory
        );
        assert_eq!(
            io_error_from_file_error(FileError::Acces),
            IOErrorEnum::PermissionDenied
        );
        assert_eq!(
            io_error_from_file_error(FileError::NameTooLong),
            IOErrorEnum::FilenameTooLong
        );
        assert_eq!(
            io_error_from_file_error(FileError::NoEnt),
            IOErrorEnum::NotFound
        );
        assert_eq!(
            io_error_from_file_error(FileError::NotDir),
            IOErrorEnum::NotDirectory
        );
        assert_eq!(
            io_error_from_file_error(FileError::Nxio),
            IOErrorEnum::NotRegularFile
        );
        assert_eq!(
            io_error_from_file_error(FileError::NoDev),
            IOErrorEnum::NoSuchDevice
        );
        assert_eq!(
            io_error_from_file_error(FileError::RoFs),
            IOErrorEnum::ReadOnly
        );
        assert_eq!(
            io_error_from_file_error(FileError::TxtBsy),
            IOErrorEnum::Busy
        );
        assert_eq!(
            io_error_from_file_error(FileError::Loop),
            IOErrorEnum::TooManyLinks
        );
        assert_eq!(
            io_error_from_file_error(FileError::NoSpc),
            IOErrorEnum::NoSpace
        );
        assert_eq!(
            io_error_from_file_error(FileError::NoMem),
            IOErrorEnum::NoSpace
        );
        assert_eq!(
            io_error_from_file_error(FileError::MFile),
            IOErrorEnum::TooManyOpenFiles
        );
        assert_eq!(
            io_error_from_file_error(FileError::NFile),
            IOErrorEnum::TooManyOpenFiles
        );
        assert_eq!(
            io_error_from_file_error(FileError::Inval),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_file_error(FileError::Pipe),
            IOErrorEnum::BrokenPipe
        );
        assert_eq!(
            io_error_from_file_error(FileError::Again),
            IOErrorEnum::WouldBlock
        );
        assert_eq!(
            io_error_from_file_error(FileError::Perm),
            IOErrorEnum::PermissionDenied
        );
        assert_eq!(
            io_error_from_file_error(FileError::NoSys),
            IOErrorEnum::NotSupported
        );
        // Failed-mapping group.
        assert_eq!(
            io_error_from_file_error(FileError::BadF),
            IOErrorEnum::Failed
        );
        assert_eq!(
            io_error_from_file_error(FileError::Failed),
            IOErrorEnum::Failed
        );
        assert_eq!(
            io_error_from_file_error(FileError::Fault),
            IOErrorEnum::Failed
        );
        assert_eq!(
            io_error_from_file_error(FileError::Intr),
            IOErrorEnum::Failed
        );
        assert_eq!(io_error_from_file_error(FileError::Io), IOErrorEnum::Failed);
    }

    #[test]
    fn from_errno_via_file_error() {
        // ENOENT -> FileError::NoEnt -> IOErrorEnum::NotFound
        assert_eq!(io_error_from_errno(2), IOErrorEnum::NotFound);
        // EEXIST -> FileError::Exist -> IOErrorEnum::Exists
        assert_eq!(io_error_from_errno(17), IOErrorEnum::Exists);
        // EACCES -> FileError::Acces -> IOErrorEnum::PermissionDenied
        assert_eq!(io_error_from_errno(13), IOErrorEnum::PermissionDenied);
        // ENOSPC -> FileError::NoSpc -> IOErrorEnum::NoSpace
        assert_eq!(io_error_from_errno(28), IOErrorEnum::NoSpace);
        // EINVAL -> FileError::Inval -> IOErrorEnum::InvalidArgument
        assert_eq!(io_error_from_errno(22), IOErrorEnum::InvalidArgument);
    }

    #[test]
    fn from_errno_additional_codes() {
        // ECANCELED -> Cancelled
        assert_eq!(io_error_from_errno(125), IOErrorEnum::Cancelled);
        // ENOTEMPTY -> NotEmpty
        assert_eq!(io_error_from_errno(39), IOErrorEnum::NotEmpty);
        // ETIMEDOUT -> TimedOut
        assert_eq!(io_error_from_errno(110), IOErrorEnum::TimedOut);
        // EBUSY -> Busy
        assert_eq!(io_error_from_errno(16), IOErrorEnum::Busy);
        // EWOULDBLOCK / EAGAIN -> WouldBlock
        assert_eq!(io_error_from_errno(11), IOErrorEnum::WouldBlock);
        // EADDRINUSE -> AddressInUse
        assert_eq!(io_error_from_errno(98), IOErrorEnum::AddressInUse);
        // EHOSTUNREACH -> HostUnreachable
        assert_eq!(io_error_from_errno(113), IOErrorEnum::HostUnreachable);
        // ENETUNREACH -> NetworkUnreachable
        assert_eq!(io_error_from_errno(101), IOErrorEnum::NetworkUnreachable);
        // ECONNREFUSED -> ConnectionRefused
        assert_eq!(io_error_from_errno(111), IOErrorEnum::ConnectionRefused);
        // ECONNRESET -> ConnectionClosed (== BrokenPipe)
        assert_eq!(io_error_from_errno(104), IOErrorEnum::CONNECTION_CLOSED);
        assert_eq!(io_error_from_errno(104), IOErrorEnum::BrokenPipe);
        // ENOTCONN -> NotConnected
        assert_eq!(io_error_from_errno(107), IOErrorEnum::NotConnected);
        // EMSGSIZE -> MessageTooLarge
        assert_eq!(io_error_from_errno(90), IOErrorEnum::MessageTooLarge);
        // ENOTSOCK -> InvalidArgument
        assert_eq!(io_error_from_errno(88), IOErrorEnum::InvalidArgument);
        // EPROTONOSUPPORT -> NotSupported
        assert_eq!(io_error_from_errno(93), IOErrorEnum::NotSupported);
        // EDESTADDRREQ -> DestinationUnset
        assert_eq!(io_error_from_errno(89), IOErrorEnum::DestinationUnset);
        // ENOMSG -> InvalidData
        assert_eq!(io_error_from_errno(42), IOErrorEnum::InvalidData);
        // EMLINK -> TooManyLinks
        assert_eq!(io_error_from_errno(31), IOErrorEnum::TooManyLinks);
    }

    #[test]
    fn from_errno_unknown_returns_failed() {
        assert_eq!(io_error_from_errno(9999), IOErrorEnum::Failed);
        assert_eq!(io_error_from_errno(0), IOErrorEnum::Failed);
    }

    #[test]
    fn from_win32_error_common_codes() {
        use super::win32_error::*;

        assert_eq!(
            io_error_from_win32_error(ERROR_FILE_NOT_FOUND),
            IOErrorEnum::NotFound
        );
        assert_eq!(
            io_error_from_win32_error(ERROR_ACCESS_DENIED),
            IOErrorEnum::PermissionDenied
        );
        assert_eq!(
            io_error_from_win32_error(ERROR_ALREADY_EXISTS),
            IOErrorEnum::Exists
        );
        assert_eq!(
            io_error_from_win32_error(WSAEADDRINUSE),
            IOErrorEnum::AddressInUse
        );
        assert_eq!(
            io_error_from_win32_error(WSAEWOULDBLOCK),
            IOErrorEnum::WouldBlock
        );
        assert_eq!(
            io_error_from_win32_error(WSAEACCES),
            IOErrorEnum::PermissionDenied
        );
        assert_eq!(
            io_error_from_win32_error(WSA_INVALID_HANDLE),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_win32_error(WSA_INVALID_PARAMETER),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_win32_error(WSAEINVAL),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_win32_error(WSAEBADF),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_win32_error(WSAENOTSOCK),
            IOErrorEnum::InvalidArgument
        );
        assert_eq!(
            io_error_from_win32_error(WSAEPROTONOSUPPORT),
            IOErrorEnum::NotSupported
        );
        assert_eq!(
            io_error_from_win32_error(WSAECANCELLED),
            IOErrorEnum::Cancelled
        );
        assert_eq!(
            io_error_from_win32_error(WSAESOCKTNOSUPPORT),
            IOErrorEnum::NotSupported
        );
        assert_eq!(
            io_error_from_win32_error(WSAEOPNOTSUPP),
            IOErrorEnum::NotSupported
        );
        assert_eq!(
            io_error_from_win32_error(WSAEPFNOSUPPORT),
            IOErrorEnum::NotSupported
        );
        assert_eq!(
            io_error_from_win32_error(WSAEAFNOSUPPORT),
            IOErrorEnum::NotSupported
        );
        assert_eq!(
            io_error_from_win32_error(WSAECONNRESET),
            IOErrorEnum::CONNECTION_CLOSED
        );
        assert_eq!(
            io_error_from_win32_error(WSAENETRESET),
            IOErrorEnum::CONNECTION_CLOSED
        );
        assert_eq!(
            io_error_from_win32_error(WSAESHUTDOWN),
            IOErrorEnum::CONNECTION_CLOSED
        );
        assert_eq!(
            io_error_from_win32_error(WSAEHOSTUNREACH),
            IOErrorEnum::HostUnreachable
        );
        assert_eq!(
            io_error_from_win32_error(WSAENETUNREACH),
            IOErrorEnum::NetworkUnreachable
        );
        assert_eq!(
            io_error_from_win32_error(WSAECONNREFUSED),
            IOErrorEnum::ConnectionRefused
        );
        assert_eq!(
            io_error_from_win32_error(WSAETIMEDOUT),
            IOErrorEnum::TimedOut
        );
        assert_eq!(
            io_error_from_win32_error(WSAENOTCONN),
            IOErrorEnum::NotConnected
        );
        assert_eq!(
            io_error_from_win32_error(ERROR_PIPE_LISTENING),
            IOErrorEnum::NotConnected
        );
        assert_eq!(
            io_error_from_win32_error(WSAEMSGSIZE),
            IOErrorEnum::MessageTooLarge
        );
        assert_eq!(io_error_from_win32_error(u32::MAX), IOErrorEnum::Failed);
        assert_eq!(io_error_from_win32_error(0), IOErrorEnum::Failed);
    }
}
