//! Compile-time assertions.
//!
//! Ported from Linux `rust/kernel/build_assert.rs` and `rust/build_error.rs`.
//!
//! Three types of build-time assertions:
//! - [`static_assert!`] — equivalent to C `static_assert`, usable outside bodies
//! - [`const_assert!`] — more powerful, can refer to generics inside functions
//! - [`build_assert!`] — most powerful, can check runtime values that the optimizer
//!   may constant-fold

/// Build-time error function. Panics in const context; triggers a linker error
/// if the optimizer cannot prove it is unreachable.
#[inline(never)]
#[cold]
#[export_name = "rust_build_error"]
#[track_caller]
pub const fn build_error(msg: &'static str) -> ! {
    panic!("{}", msg);
}

/// Static assert (i.e. compile-time assert), equivalent to C11 `_Static_assert`.
///
/// Cannot refer to generics or variables. Usable outside function bodies.
///
/// # Examples
/// ```
/// static_assert!(core::mem::size_of::<u8>() == 1);
/// ```
#[macro_export]
macro_rules! static_assert {
    ($condition:expr $(,$arg:literal)?) => {
        const _: () = ::core::assert!($condition $(,$arg)?);
    };
}

/// Assertion during constant evaluation. Can refer to generics but not variables.
///
/// # Examples
/// ```
/// fn foo<const N: usize>() {
///     const_assert!(N > 1);
/// }
/// ```
#[macro_export]
macro_rules! const_assert {
    ($condition:expr $(,$arg:literal)?) => {
        const { ::core::assert!($condition $(,$arg)?) };
    };
}

/// Build-time error macro. Fails the build if the code path can be reached.
#[macro_export]
macro_rules! build_error {
    () => {{
        $crate::linux_rust::build_assert::build_error("")
    }};
    ($msg:expr) => {{
        $crate::linux_rust::build_assert::build_error($msg)
    }};
}

/// Asserts a boolean expression is `true` at compile time.
///
/// More powerful than `static_assert!` and `const_assert!` — can check
/// runtime values if the optimizer can constant-fold them.
///
/// # Examples
/// ```
/// #[inline(always)]
/// fn bar(n: usize) {
///     build_assert!(n > 1);
/// }
/// ```
#[macro_export]
macro_rules! build_assert {
    ($cond:expr $(,)?) => {{
        if !$cond {
            $crate::linux_rust::build_assert::build_error(concat!(
                "assertion failed: ",
                stringify!($cond)
            ));
        }
    }};
    ($cond:expr, $msg:expr) => {{
        if !$cond {
            $crate::linux_rust::build_assert::build_error($msg);
        }
    }};
}
