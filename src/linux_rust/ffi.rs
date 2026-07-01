//! Foreign function interface (FFI) types.
//!
//! Ported from Linux `rust/ffi.rs`.
//!
//! Maps C primitive types to Rust ones. Uses `core::ffi` for standard types.

pub use core::ffi::c_void;
pub use core::ffi::CStr;

pub type c_char = u8;
pub type c_schar = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_ushort = u16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = isize;
pub type c_ulong = usize;
pub type c_longlong = i64;
pub type c_ulonglong = u64;

const _: () = assert!(core::mem::size_of::<c_char>() == 1);
const _: () = assert!(core::mem::size_of::<c_int>() == 4);
const _: () = assert!(core::mem::size_of::<c_long>() == core::mem::size_of::<isize>());
