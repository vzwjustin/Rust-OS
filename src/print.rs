//! Print macros for kernel output

use core::fmt;

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    // Ignore write errors: once the desktop switches to a graphics-mode
    // framebuffer the VGA text writer can fail, and a debug print must never
    // panic the kernel.
    let _ = crate::vga_buffer::VGA_WRITER.lock().write_fmt(args);
}
