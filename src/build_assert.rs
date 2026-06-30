// SPDX-License-Identifier: GPL-2.0
//! Compile-time assertions — ported from Linux `rust/kernel/build_assert.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

/// Fails the build if the code path calling `build_error!` can possibly be executed.
///
/// If executed in const context, panics.  If the compiler cannot prove the path is
/// unreachable, the link fails with an undefined-symbol error.
#[macro_export]
macro_rules! build_error {
    () => {{
        build_error!("explicit build_error!")
    }};
    ($msg:expr) => {{
        // In const context this panics immediately.
        // In non-const context the call to an extern fn that is never defined
        // causes a linker error if the optimizer cannot eliminate the call.
        extern "Rust" {
            #[link_name = concat!("build_error: ", $msg)]
            fn trigger() -> !;
        }
        // SAFETY: this call is intentionally never reachable at runtime.
        #[allow(unused_unsafe)]
        unsafe { trigger() }
    }};
}

/// Static (module-level) compile-time assertion.
///
/// Cannot refer to generics or runtime values.
#[macro_export]
macro_rules! static_assert {
    ($condition:expr $(, $msg:literal)?) => {
        const _: () = ::core::assert!($condition $(, $msg)?);
    };
}

/// Assertion inside a function body that may refer to generics but not to runtime values.
#[macro_export]
macro_rules! const_assert {
    ($condition:expr $(, $msg:literal)?) => {
        const { ::core::assert!($condition $(, $msg)?) };
    };
}

/// Assertion that may refer to runtime values.  Falls back to a linker error when the
/// condition cannot be proved at compile time.
///
/// The containing function **must** be `#[inline(always)]` for the optimizer to be able
/// to eliminate the error path.
#[macro_export]
macro_rules! build_assert {
    ($cond:expr $(,)?) => {{
        if !$cond {
            $crate::build_error!(concat!("assertion failed: ", stringify!($cond)));
        }
    }};
    ($cond:expr, $msg:expr) => {{
        if !$cond {
            $crate::build_error!($msg);
        }
    }};
}
