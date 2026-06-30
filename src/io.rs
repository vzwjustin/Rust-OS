// SPDX-License-Identifier: MIT
// RustOS MMIO + port I/O + memory barriers (ported from Linux rust/kernel/io.rs)
// All bindings:: calls replaced with native x86_64 inline asm.

#![allow(dead_code, unused_variables)]

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

// ---------------------------------------------------------------------------
// MMIO — volatile read / write
// ---------------------------------------------------------------------------

/// Read a byte from MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address.
#[inline]
pub unsafe fn ioread8(addr: *const u8) -> u8 {
    unsafe { read_volatile(addr) }
}

/// Read a 16-bit word from MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 2 bytes.
#[inline]
pub unsafe fn ioread16(addr: *const u16) -> u16 {
    unsafe { read_volatile(addr) }
}

/// Read a 32-bit word from MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 4 bytes.
#[inline]
pub unsafe fn ioread32(addr: *const u32) -> u32 {
    unsafe { read_volatile(addr) }
}

/// Read a 64-bit word from MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 8 bytes.
#[inline]
pub unsafe fn ioread64(addr: *const u64) -> u64 {
    unsafe { read_volatile(addr) }
}

/// Write a byte to MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address.
#[inline]
pub unsafe fn iowrite8(val: u8, addr: *mut u8) {
    unsafe { write_volatile(addr, val) }
}

/// Write a 16-bit word to MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 2 bytes.
#[inline]
pub unsafe fn iowrite16(val: u16, addr: *mut u16) {
    unsafe { write_volatile(addr, val) }
}

/// Write a 32-bit word to MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 4 bytes.
#[inline]
pub unsafe fn iowrite32(val: u32, addr: *mut u32) {
    unsafe { write_volatile(addr, val) }
}

/// Write a 64-bit word to MMIO address `addr`.
///
/// # Safety
/// `addr` must be a valid, mapped MMIO address aligned to 8 bytes.
#[inline]
pub unsafe fn iowrite64(val: u64, addr: *mut u64) {
    unsafe { write_volatile(addr, val) }
}

// ---------------------------------------------------------------------------
// x86 Port I/O
// ---------------------------------------------------------------------------

/// Read a byte from I/O port `port` (x86 `inb`).
///
/// # Safety
/// Requires appropriate I/O privilege level (CPL=0 or IOPL).
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") val,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}

/// Read a 16-bit word from I/O port `port` (x86 `inw`).
///
/// # Safety
/// Requires ring-0 or IOPL.
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!(
            "in ax, dx",
            in("dx") port,
            out("ax") val,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}

/// Read a 32-bit word from I/O port `port` (x86 `inl`).
///
/// # Safety
/// Requires ring-0 or IOPL.
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        asm!(
            "in eax, dx",
            in("dx") port,
            out("eax") val,
            options(nomem, nostack, preserves_flags),
        );
    }
    val
}

/// Write a byte to I/O port `port` (x86 `outb`).
///
/// # Safety
/// Requires ring-0 or IOPL.
#[inline]
pub unsafe fn outb(val: u8, port: u16) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Write a 16-bit word to I/O port `port` (x86 `outw`).
///
/// # Safety
/// Requires ring-0 or IOPL.
#[inline]
pub unsafe fn outw(val: u16, port: u16) {
    unsafe {
        asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Write a 32-bit word to I/O port `port` (x86 `outl`).
///
/// # Safety
/// Requires ring-0 or IOPL.
#[inline]
pub unsafe fn outl(val: u32, port: u16) {
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") val,
            options(nomem, nostack, preserves_flags),
        );
    }
}

// ---------------------------------------------------------------------------
// Memory barriers (x86_64)
// ---------------------------------------------------------------------------
// On x86_64, loads are never reordered with other loads, and stores are
// never reordered with other stores. The main barrier needed is `mfence`
// (or `lock addl`) for store-load ordering.

/// Full memory barrier (serialises all prior loads and stores).
#[inline]
pub fn mb() {
    unsafe {
        asm!("mfence", options(nostack, preserves_flags));
    }
}

/// Read (load) memory barrier.
/// On x86 this is a no-op (loads are not reordered), but we emit `lfence`
/// to prevent speculative load reordering.
#[inline]
pub fn rmb() {
    unsafe {
        asm!("lfence", options(nostack, preserves_flags));
    }
}

/// Write (store) memory barrier.
/// On x86 this is a no-op (stores are not reordered), but we emit `sfence`
/// for store-buffer flushing (relevant for WC/WT memory types).
#[inline]
pub fn wmb() {
    unsafe {
        asm!("sfence", options(nostack, preserves_flags));
    }
}

/// SMP full memory barrier.
/// Equivalent to `mb()` on x86.
#[inline]
pub fn smp_mb() {
    mb();
}

/// SMP read memory barrier.
/// A compiler barrier suffices on x86 (CPU TSO guarantees load ordering);
/// we still emit `lfence` for correctness with non-temporal loads.
#[inline]
pub fn smp_rmb() {
    rmb();
}

/// SMP write memory barrier.
/// A compiler barrier suffices on x86 (CPU TSO guarantees store ordering);
/// we still emit `sfence` for WC memory correctness.
#[inline]
pub fn smp_wmb() {
    wmb();
}

/// Compiler-only barrier (prevents reordering by the optimiser; no CPU fence).
#[inline]
pub fn barrier() {
    unsafe {
        asm!("", options(nostack, preserves_flags));
    }
}

// ---------------------------------------------------------------------------
// IoRegister — typed MMIO register accessor
// ---------------------------------------------------------------------------

/// A typed MMIO register at a fixed virtual address.
pub struct IoRegister<T: Copy> {
    ptr: *mut T,
}

// SAFETY: MMIO registers are typically accessed from a single hart or with
// external synchronisation; the caller is responsible for thread safety.
unsafe impl<T: Copy + Send> Send for IoRegister<T> {}
unsafe impl<T: Copy + Sync> Sync for IoRegister<T> {}

impl<T: Copy> IoRegister<T> {
    /// Construct from a kernel virtual address.
    ///
    /// # Safety
    /// `addr` must be a valid MMIO virtual address, suitably aligned for `T`,
    /// and the mapping must outlive `Self`.
    #[inline]
    pub unsafe fn new(addr: usize) -> Self {
        IoRegister { ptr: addr as *mut T }
    }

    /// Volatile read of the register value.
    ///
    /// # Safety
    /// The MMIO region must be accessible.
    #[inline]
    pub unsafe fn read(&self) -> T {
        unsafe { read_volatile(self.ptr) }
    }

    /// Volatile write to the register.
    ///
    /// # Safety
    /// The MMIO region must be accessible and writable.
    #[inline]
    pub unsafe fn write(&self, val: T) {
        unsafe { write_volatile(self.ptr, val) }
    }

    /// Read-modify-write: apply `f` to the current value and write back.
    ///
    /// # Safety
    /// Same as `read` and `write`.
    #[inline]
    pub unsafe fn modify<F: FnOnce(T) -> T>(&self, f: F) {
        let old = unsafe { self.read() };
        unsafe { self.write(f(old)) };
    }
}

// ---------------------------------------------------------------------------
// IoMem — a mapped MMIO region (also lives in dma.rs, re-exported here)
// ---------------------------------------------------------------------------

/// A contiguous MMIO region mapped into the kernel virtual address space.
pub struct IoMem {
    base: *mut u8,
    size: usize,
}

// SAFETY: The caller guarantees exclusive access via the `unsafe` constructor.
unsafe impl Send for IoMem {}

impl IoMem {
    /// Map physical address `phys` for `size` bytes.
    ///
    /// # Safety
    /// `phys` must be a valid MMIO physical address and `size` must not exceed
    /// the device's BAR region.  The mapping is created via identity mapping on
    /// early-boot RustOS; replace with a proper `ioremap` when virtual memory
    /// is active.
    pub unsafe fn new(phys: u64, size: usize) -> Result<Self, i32> {
        // In early-boot identity-mapped mode phys == virt.
        // A real kernel would call ioremap() here.
        Ok(IoMem { base: phys as *mut u8, size })
    }

    /// Raw base pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.base
    }

    /// Size of the mapping in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    unsafe fn check_offset(&self, offset: usize, width: usize) {
        debug_assert!(offset + width <= self.size, "IoMem: offset out of range");
    }

    /// Read a byte at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping.
    #[inline]
    pub unsafe fn read_u8(&self, offset: usize) -> u8 {
        unsafe { self.check_offset(offset, 1); ioread8(self.base.add(offset)) }
    }

    /// Read a 16-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 2 bytes.
    #[inline]
    pub unsafe fn read_u16(&self, offset: usize) -> u16 {
        unsafe { self.check_offset(offset, 2); ioread16(self.base.add(offset) as *const u16) }
    }

    /// Read a 32-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 4 bytes.
    #[inline]
    pub unsafe fn read_u32(&self, offset: usize) -> u32 {
        unsafe { self.check_offset(offset, 4); ioread32(self.base.add(offset) as *const u32) }
    }

    /// Read a 64-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 8 bytes.
    #[inline]
    pub unsafe fn read_u64(&self, offset: usize) -> u64 {
        unsafe { self.check_offset(offset, 8); ioread64(self.base.add(offset) as *const u64) }
    }

    /// Write a byte at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping.
    #[inline]
    pub unsafe fn write_u8(&self, offset: usize, val: u8) {
        unsafe { self.check_offset(offset, 1); iowrite8(val, self.base.add(offset)) }
    }

    /// Write a 16-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 2 bytes.
    #[inline]
    pub unsafe fn write_u16(&self, offset: usize, val: u16) {
        unsafe { self.check_offset(offset, 2); iowrite16(val, self.base.add(offset) as *mut u16) }
    }

    /// Write a 32-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 4 bytes.
    #[inline]
    pub unsafe fn write_u32(&self, offset: usize, val: u32) {
        unsafe { self.check_offset(offset, 4); iowrite32(val, self.base.add(offset) as *mut u32) }
    }

    /// Write a 64-bit word at `offset`.
    ///
    /// # Safety
    /// `offset` must be within the mapping and aligned to 8 bytes.
    #[inline]
    pub unsafe fn write_u64(&self, offset: usize, val: u64) {
        unsafe { self.check_offset(offset, 8); iowrite64(val, self.base.add(offset) as *mut u64) }
    }
}
