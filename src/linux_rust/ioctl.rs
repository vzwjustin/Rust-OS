//! `ioctl()` number definitions.
//!
//! Ported from Linux `rust/kernel/ioctl.rs`.
//! C header: `include/asm-generic/ioctl.h`

#![allow(non_snake_case)]

// ── ioctl direction constants ───────────────────────────────────────────

pub const IOC_NONE: u32 = 0;
pub const IOC_WRITE: u32 = 1;
pub const IOC_READ: u32 = 2;

// ── ioctl field widths ──────────────────────────────────────────────────

pub const IOC_NRBITS: u32 = 8;
pub const IOC_TYPEBITS: u32 = 8;
pub const IOC_SIZEBITS: u32 = 14;
pub const IOC_DIRBITS: u32 = 2;

// ── ioctl field masks ───────────────────────────────────────────────────

pub const IOC_NRMASK: u32 = (1 << IOC_NRBITS) - 1;
pub const IOC_TYPEMASK: u32 = (1 << IOC_TYPEBITS) - 1;
pub const IOC_SIZEMASK: u32 = (1 << IOC_SIZEBITS) - 1;
pub const IOC_DIRMASK: u32 = (1 << IOC_DIRBITS) - 1;

// ── ioctl field shifts ──────────────────────────────────────────────────

pub const IOC_NRSHIFT: u32 = 0;
pub const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
pub const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
pub const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;

/// Build an ioctl number, analogous to the C macro `_IOC`.
#[inline(always)]
const fn _IOC(dir: u32, ty: u32, nr: u32, size: usize) -> u32 {
    assert!(dir <= IOC_DIRMASK);
    assert!(ty <= IOC_TYPEMASK);
    assert!(nr <= IOC_NRMASK);
    assert!(size <= (IOC_SIZEMASK as usize));

    (dir << IOC_DIRSHIFT)
        | (ty << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | ((size as u32) << IOC_SIZESHIFT)
}

/// Build an ioctl number for an argumentless ioctl.
#[inline(always)]
pub const fn _IO(ty: u32, nr: u32) -> u32 {
    _IOC(IOC_NONE, ty, nr, 0)
}

/// Build an ioctl number for a read-only ioctl.
#[inline(always)]
pub const fn _IOR<T>(ty: u32, nr: u32) -> u32 {
    _IOC(IOC_READ, ty, nr, core::mem::size_of::<T>())
}

/// Build an ioctl number for a write-only ioctl.
#[inline(always)]
pub const fn _IOW<T>(ty: u32, nr: u32) -> u32 {
    _IOC(IOC_WRITE, ty, nr, core::mem::size_of::<T>())
}

/// Build an ioctl number for a read-write ioctl.
#[inline(always)]
pub const fn _IOWR<T>(ty: u32, nr: u32) -> u32 {
    _IOC(IOC_READ | IOC_WRITE, ty, nr, core::mem::size_of::<T>())
}

/// Get the ioctl direction from an ioctl number.
pub const fn _IOC_DIR(nr: u32) -> u32 {
    (nr >> IOC_DIRSHIFT) & IOC_DIRMASK
}

/// Get the ioctl type from an ioctl number.
pub const fn _IOC_TYPE(nr: u32) -> u32 {
    (nr >> IOC_TYPESHIFT) & IOC_TYPEMASK
}

/// Get the ioctl number from an ioctl number.
pub const fn _IOC_NR(nr: u32) -> u32 {
    (nr >> IOC_NRSHIFT) & IOC_NRMASK
}

/// Get the ioctl size from an ioctl number.
pub const fn _IOC_SIZE(nr: u32) -> usize {
    ((nr >> IOC_SIZESHIFT) & IOC_SIZEMASK) as usize
}
