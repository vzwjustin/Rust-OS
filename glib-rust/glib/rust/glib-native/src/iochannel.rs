//! I/O channel types matching `giochannel.h`.
//!
//! Defines error types, status, seek types, and flags for I/O channels.
//! The actual channel implementation requires OS file/pipe support and
//! is deferred. Fully `no_std` compatible.

/// I/O error (`GIOError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IOError {
    None,
    Again,
    Inval,
    Unknown,
}

/// I/O channel error (`GIOChannelError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IOChannelError {
    Fbig,
    Inval,
    Io,
    Isdir,
    Nospc,
    Nxio,
    Overflow,
    Pipe,
    Failed,
}

/// I/O status (`GIOStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IOStatus {
    Error,
    Normal,
    Eof,
    Again,
}

/// Seek type (`GSeekType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeekType {
    Cur,
    Set,
    End,
}

/// I/O flags (`GIOFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IOFlags(pub u32);

impl IOFlags {
    pub const NONE: IOFlags = IOFlags(0);
    pub const APPEND: IOFlags = IOFlags(1 << 0);
    pub const NONBLOCK: IOFlags = IOFlags(1 << 1);
    pub const IS_READABLE: IOFlags = IOFlags(1 << 2);
    pub const IS_WRITABLE: IOFlags = IOFlags(1 << 3);
    pub const IS_SEEKABLE: IOFlags = IOFlags(1 << 4);
    pub const MASK: IOFlags = IOFlags((1 << 5) - 1);
    pub const GET_MASK: IOFlags = IOFlags((1 << 5) - 1);
    pub const SET_MASK: IOFlags = IOFlags((1 << 0) | (1 << 1));

    pub fn contains(self, other: IOFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(self, other: IOFlags) -> IOFlags {
        IOFlags(self.0 | other.0)
    }

    pub fn remove(self, other: IOFlags) -> IOFlags {
        IOFlags(self.0 & !other.0)
    }
}

impl core::ops::BitOr for IOFlags {
    type Output = IOFlags;
    fn bitor(self, rhs: IOFlags) -> IOFlags {
        IOFlags(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for IOFlags {
    type Output = IOFlags;
    fn bitand(self, rhs: IOFlags) -> IOFlags {
        IOFlags(self.0 & rhs.0)
    }
}

/// I/O channel error quark (`g_io_channel_error_quark`).
pub fn io_channel_error_quark() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_variants() {
        assert_ne!(IOError::None, IOError::Again);
        assert_ne!(IOError::Inval, IOError::Unknown);
    }

    #[test]
    fn io_status_variants() {
        assert_eq!(IOStatus::Normal, IOStatus::Normal);
        assert_ne!(IOStatus::Eof, IOStatus::Error);
    }

    #[test]
    fn seek_type() {
        assert_ne!(SeekType::Set, SeekType::Cur);
        assert_ne!(SeekType::End, SeekType::Set);
    }

    #[test]
    fn io_flags() {
        let flags = IOFlags::APPEND | IOFlags::NONBLOCK;
        assert!(flags.contains(IOFlags::APPEND));
        assert!(flags.contains(IOFlags::NONBLOCK));
        assert!(!flags.contains(IOFlags::IS_READABLE));
    }

    #[test]
    fn io_flags_set_mask() {
        let set = IOFlags::SET_MASK;
        assert!(set.contains(IOFlags::APPEND));
        assert!(set.contains(IOFlags::NONBLOCK));
        assert!(!set.contains(IOFlags::IS_READABLE));
    }
}
