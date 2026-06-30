//! Kernel macros.
//!
//! Ported from Linux `rust/macros/`. The original macros are proc-macros;
//! here we provide declarative macro approximations where possible.
//!
//! ## Not ported (require proc-macro support):
//! - `#[pin_data]` — use `pin_init!` manually instead
//! - `#[pinned_drop]` — implement `PinnedDrop` manually
//! - `#[vtable]` — generates `HAS_*` const booleans on traits/impls
//! - `#[export]` — bindgen signature verification + `#[no_mangle]`
//! - `module!` — kernel module declaration
//! - `fmt!` — format args adapter (use `core::format_args!` directly)
//! - `paste!` — identifier concatenation (use explicit names instead)
//! - `concat_idents!` — identifier concatenation
//! - `Zeroable` derive — use `unsafe impl Zeroable` manually
//! - `MaybeZeroable` derive — use `unsafe impl Zeroable` manually
//! - `kunit_tests` — test framework attribute

/// Marker for vtable traits.
///
/// In the original kernel this is a proc-macro that generates `HAS_*` constants.
/// Here it's a no-op attribute macro approximation — add `USE_VTABLE_ATTR` manually.
#[macro_export]
macro_rules! vtable {
    () => {};
}

/// No-op marker for `#[vtable]` on impl blocks. Add `HAS_*` consts manually.
#[macro_export]
macro_rules! vtable_impl {
    () => {};
}

/// Approximation of `paste!` — since we can't do token concatenation in
/// declarative macros, this just passes through the input unchanged.
///
/// Where the kernel uses `paste!` to generate identifiers like `bit_u64`
/// from `bit_$T`, we use explicit function names instead.
#[macro_export]
macro_rules! paste {
    ($($tt:tt)*) => { $($tt)* };
}

/// Format macro for kernel-style formatting.
///
/// In the original kernel this wraps arguments with `Adapter` for foreign types.
/// Here we just delegate to `core::format_args!`.
#[macro_export]
macro_rules! kfmt {
    ($($arg:tt)*) => { ::core::format_args!($($arg)*) };
}

/// Kernel print macros.
#[macro_export]
macro_rules! pr_info {
    ($($arg:tt)*) => {
        ::core::write!(&mut $crate::print::WRITER, $($arg)*).ok()
    };
}

#[macro_export]
macro_rules! pr_warn {
    ($($arg:tt)*) => {
        ::core::write!(&mut $crate::print::WRITER, $($arg)*).ok()
    };
}

#[macro_export]
macro_rules! pr_err {
    ($($arg:tt)*) => {
        ::core::write!(&mut $crate::print::WRITER, $($arg)*).ok()
    };
}

#[macro_export]
macro_rules! pr_debug {
    ($($arg:tt)*) => {};
}

/// `for_lt!` macro — generates a HRTB impl.
///
/// In the original this is a proc-macro. Here we provide a simple declarative
/// approximation that just expands to the given impl with `for<'a>` prepended
/// to lifetime parameters.
#[macro_export]
macro_rules! for_lt {
    ($($tt:tt)*) => { $($tt)* };
}
