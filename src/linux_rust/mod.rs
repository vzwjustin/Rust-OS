//! Linux kernel Rust abstractions ported to RustOS.
//!
//! These modules are adapted from the Linux kernel's `rust/kernel/` directory,
//! with dependencies on Linux C bindings (`crate::bindings`) and proc macros
//! (`crate::macros`) replaced by standalone Rust implementations suitable
//! for RustOS's `no_std` environment.
//!
//! Ported modules:
//! - `build_assert`: Compile-time assertions (`static_assert!`, `const_assert!`, `build_assert!`)
//! - `sizes`: Common size constants (`SZ_1K`, `SZ_4M`, etc.) + `SizeConstants` trait
//! - `bits`: Bit manipulation (`bit_u64`, `genmask_u64`, etc.)
//! - `ioctl`: IOCTL number builders (`_IO`, `_IOR`, `_IOW`, `_IOWR`)
//! - `num`: Integer type trait
//! - `ffi`: C FFI type aliases
//! - `fmt`: Formatting helpers
//! - `bitmap`: Bitmap operations
//! - `bitfield`: Bitfield register operations

pub mod bitfield;
pub mod bitmap;
pub mod bits;
pub mod bounded;
pub mod build_assert;
pub mod ffi;
pub mod fmt;
pub mod ioctl;
pub mod num;
pub mod sizes;
