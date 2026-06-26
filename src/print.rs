//! Print macros for kernel output

use core::fmt;

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    crate::vga_buffer::VGA_WRITER
        .lock()
        .write_fmt(args)
        .unwrap();
}
