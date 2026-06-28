//! Win32 utility compatibility (`gwin32.c`).

pub const ERROR_SUCCESS: u32 = 0;
pub const ERROR_FILE_NOT_FOUND: u32 = 2;
pub const ERROR_PATH_NOT_FOUND: u32 = 3;
pub const ERROR_ACCESS_DENIED: u32 = 5;
pub const ERROR_INVALID_HANDLE: u32 = 6;
pub const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
pub const ERROR_INVALID_DATA: u32 = 13;
pub const ERROR_INVALID_DRIVE: u32 = 15;
pub const ERROR_NO_MORE_FILES: u32 = 18;
pub const ERROR_NOT_READY: u32 = 21;
pub const ERROR_SHARING_VIOLATION: u32 = 32;
pub const ERROR_HANDLE_EOF: u32 = 38;
pub const ERROR_NOT_SUPPORTED: u32 = 50;
pub const ERROR_FILE_EXISTS: u32 = 80;
pub const ERROR_INVALID_PARAMETER: u32 = 87;
pub const ERROR_BROKEN_PIPE: u32 = 109;
pub const ERROR_ALREADY_EXISTS: u32 = 183;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Win32ErrorKind {
    Success,
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    BrokenPipe,
    Unsupported,
    OutOfMemory,
    Other,
}

#[must_use]
pub const fn error_kind(code: u32) -> Win32ErrorKind {
    match code {
        ERROR_SUCCESS => Win32ErrorKind::Success,
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_INVALID_DRIVE | ERROR_NO_MORE_FILES => {
            Win32ErrorKind::NotFound
        }
        ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION => Win32ErrorKind::PermissionDenied,
        ERROR_FILE_EXISTS | ERROR_ALREADY_EXISTS => Win32ErrorKind::AlreadyExists,
        ERROR_INVALID_HANDLE | ERROR_INVALID_DATA | ERROR_INVALID_PARAMETER => {
            Win32ErrorKind::InvalidInput
        }
        ERROR_BROKEN_PIPE | ERROR_HANDLE_EOF => Win32ErrorKind::BrokenPipe,
        ERROR_NOT_SUPPORTED | ERROR_NOT_READY => Win32ErrorKind::Unsupported,
        ERROR_NOT_ENOUGH_MEMORY => Win32ErrorKind::OutOfMemory,
        _ => Win32ErrorKind::Other,
    }
}

#[must_use]
pub const fn error_message(code: u32) -> &'static str {
    match error_kind(code) {
        Win32ErrorKind::Success => "success",
        Win32ErrorKind::NotFound => "not found",
        Win32ErrorKind::PermissionDenied => "permission denied",
        Win32ErrorKind::AlreadyExists => "already exists",
        Win32ErrorKind::InvalidInput => "invalid input",
        Win32ErrorKind::BrokenPipe => "broken pipe",
        Win32ErrorKind::Unsupported => "operation not supported",
        Win32ErrorKind::OutOfMemory => "not enough memory",
        Win32ErrorKind::Other => "unknown Win32 error",
    }
}

#[must_use]
pub const fn succeeded(code: u32) -> bool {
    code == ERROR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_errors() {
        assert!(succeeded(ERROR_SUCCESS));
        assert_eq!(error_kind(ERROR_FILE_NOT_FOUND), Win32ErrorKind::NotFound);
        assert_eq!(
            error_kind(ERROR_ACCESS_DENIED),
            Win32ErrorKind::PermissionDenied
        );
        assert_eq!(error_message(ERROR_BROKEN_PIPE), "broken pipe");
    }
}
