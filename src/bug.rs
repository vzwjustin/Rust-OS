// SPDX-License-Identifier: GPL-2.0
//! BUG/WARN macros — ported from Linux `rust/kernel/bug.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

/// Unconditional kernel BUG — panics immediately with a message and source location.
#[macro_export]
macro_rules! BUG {
    () => {
        panic!("BUG! at {}:{}", file!(), line!())
    };
    ($msg:expr) => {
        panic!("BUG: {} at {}:{}", $msg, file!(), line!())
    };
}

/// Conditional BUG — panics if `$cond` evaluates to `true`.
#[macro_export]
macro_rules! BUG_ON {
    ($cond:expr) => {
        if $cond {
            panic!("BUG: {} at {}:{}", stringify!($cond), file!(), line!());
        }
    };
}

/// Emit a warning if `$cond` is true; evaluates to the boolean value of `$cond`.
///
/// In release builds the warning is printed but execution continues.
#[macro_export]
macro_rules! WARN_ON {
    ($cond:expr) => {{
        let c: bool = $cond;
        if c {
            // In a real kernel we'd call the warn slow-path; here we use a debug print.
            #[cfg(debug_assertions)]
            crate::serial_println!(
                "WARNING: {} at {}:{} triggered",
                stringify!($cond),
                file!(),
                line!()
            );
        }
        c
    }};
}

/// Like [`WARN_ON`] but only fires once per call site.
#[macro_export]
macro_rules! WARN_ON_ONCE {
    ($cond:expr) => {{
        use core::sync::atomic::{AtomicBool, Ordering};
        static FIRED: AtomicBool = AtomicBool::new(false);
        let c: bool = $cond;
        if c && !FIRED.swap(true, Ordering::Relaxed) {
            #[cfg(debug_assertions)]
            crate::serial_println!(
                "WARNING (once): {} at {}:{} triggered",
                stringify!($cond),
                file!(),
                line!()
            );
        }
        c
    }};
}
