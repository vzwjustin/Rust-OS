//! Printf wrappers matching `gprintf.h` / `gprintf.c`.
//!
//! In Rust, `format!` and `write!` replace printf. These wrappers
//! provide compatibility shims. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// Format a string and return it (`g_sprintf` / `g_vasprintf`).
///
/// In Rust, just use `format!`. This is a compatibility wrapper.
pub fn sprintf(format: &str) -> String {
    // In no_std, we can't parse printf format strings.
    // This is a simple pass-through for plain strings.
    format.to_owned()
}

/// Write formatted output to a string buffer (`g_vsprintf`).
///
/// Returns the number of bytes written.
pub fn vsprintf(buf: &mut String, format: &str) -> i32 {
    buf.clear();
    buf.push_str(format);
    format.len() as i32
}

/// Printf format specifier types.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrintfArg {
    Int(i64),
    UInt(u64),
    Float(f64),
    String(&'static str),
    Char(char),
    Percent,
}

/// Simple printf-like formatter for no_std.
///
/// Supports `%d`, `%u`, `%x`, `%s`, `%c`, `%%`, `%f` with optional width.
/// This is a minimal implementation for compatibility.
pub fn printf_format(format: &str, args: &[PrintfArg]) -> String {
    let mut result = String::new();
    let mut chars = format.chars().peekable();
    let mut arg_idx = 0;

    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }

        // Parse format specifier
        let mut width = 0;
        let mut has_width = false;

        // Optional width
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                width = width * 10 + (d as u32 - '0' as u32) as usize;
                has_width = true;
                chars.next();
            } else {
                break;
            }
        }

        let spec = chars.next();
        match spec {
            Some('d') => {
                if arg_idx < args.len() {
                    if let PrintfArg::Int(v) = args[arg_idx] {
                        let s = format!("{}", v);
                        if has_width && s.len() < width {
                            for _ in 0..(width - s.len()) {
                                result.push(' ');
                            }
                        }
                        result.push_str(&s);
                        arg_idx += 1;
                    }
                }
            }
            Some('u') => {
                if arg_idx < args.len() {
                    if let PrintfArg::UInt(v) = args[arg_idx] {
                        result.push_str(&format!("{}", v));
                        arg_idx += 1;
                    }
                }
            }
            Some('x') => {
                if arg_idx < args.len() {
                    if let PrintfArg::UInt(v) = args[arg_idx] {
                        result.push_str(&format!("{:x}", v));
                        arg_idx += 1;
                    }
                }
            }
            Some('s') => {
                if arg_idx < args.len() {
                    match args[arg_idx] {
                        PrintfArg::String(s) => {
                            result.push_str(s);
                            arg_idx += 1;
                        }
                        _ => {}
                    }
                }
            }
            Some('c') => {
                if arg_idx < args.len() {
                    if let PrintfArg::Char(ch) = args[arg_idx] {
                        result.push(ch);
                        arg_idx += 1;
                    }
                }
            }
            Some('f') => {
                if arg_idx < args.len() {
                    if let PrintfArg::Float(v) = args[arg_idx] {
                        result.push_str(&format!("{}", v));
                        arg_idx += 1;
                    }
                }
            }
            Some('%') => {
                result.push('%');
            }
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprintf_basic() {
        assert_eq!(sprintf("hello"), "hello");
    }

    #[test]
    fn vsprintf_basic() {
        let mut buf = String::new();
        let n = vsprintf(&mut buf, "test");
        assert_eq!(n, 4);
        assert_eq!(buf, "test");
    }

    #[test]
    fn printf_format_int() {
        let result = printf_format("value: %d", &[PrintfArg::Int(42)]);
        assert_eq!(result, "value: 42");
    }

    #[test]
    fn printf_format_string() {
        let result = printf_format("hello %s!", &[PrintfArg::String("world")]);
        assert_eq!(result, "hello world!");
    }

    #[test]
    fn printf_format_percent() {
        let result = printf_format("100%%", &[]);
        assert_eq!(result, "100%");
    }

    #[test]
    fn printf_format_hex() {
        let result = printf_format("0x%x", &[PrintfArg::UInt(255)]);
        assert_eq!(result, "0xff");
    }

    #[test]
    fn printf_format_multiple() {
        let result = printf_format("%s = %d", &[
            PrintfArg::String("x"),
            PrintfArg::Int(10),
        ]);
        assert_eq!(result, "x = 10");
    }

    #[test]
    fn printf_format_char() {
        let result = printf_format("letter: %c", &[PrintfArg::Char('A')]);
        assert_eq!(result, "letter: A");
    }
}
