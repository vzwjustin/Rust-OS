// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Vendored items from the Rust standard library, adapted for `no_std` kernel use.
//! Ported from Linux `rust/kernel/std_vendor.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

/// Kernel equivalent of [`std::dbg!`].
///
/// Prints the source location and the `Debug` representation of each expression,
/// then returns the value unchanged.  Useful for quick-and-dirty debugging.
///
/// Unlike `std::dbg!`, this uses `crate::serial_println!` so the output goes to
/// the serial console rather than stderr.
///
/// # Examples
///
/// ```rust,ignore
/// let x = 5;
/// let y = dbg!(x * 2) + 1;  // prints "[src/lib.rs:3] x * 2 = 10"
/// assert_eq!(y, 11);
/// ```
#[macro_export]
macro_rules! dbg {
    // No-argument form: just print a location marker.
    () => {
        crate::serial_println!("[{}:{}]", file!(), line!())
    };
    // Single expression: print and return.
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                crate::serial_println!(
                    "[{}:{}] {} = {:?}",
                    file!(),
                    line!(),
                    stringify!($val),
                    &tmp,
                );
                tmp
            }
        }
    };
    // Multiple expressions: recurse.
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
