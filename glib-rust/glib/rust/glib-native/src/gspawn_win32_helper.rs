//! Win32 spawn helper compatibility (`gspawn-win32-helper.c`).

use alloc::string::String;
use alloc::vec::Vec;

#[must_use]
pub fn quote_windows_argument(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || c == '"') {
        let mut out = String::from("\"");
        let mut backslashes = 0usize;
        for ch in arg.chars() {
            match ch {
                '\\' => backslashes += 1,
                '"' => {
                    for _ in 0..(backslashes * 2 + 1) {
                        out.push('\\');
                    }
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    for _ in 0..backslashes {
                        out.push('\\');
                    }
                    backslashes = 0;
                    out.push(ch);
                }
            }
        }
        for _ in 0..(backslashes * 2) {
            out.push('\\');
        }
        out.push('"');
        out
    } else {
        String::from(arg)
    }
}

#[must_use]
pub fn join_command_line(args: &[&str]) -> String {
    let mut quoted: Vec<String> = Vec::with_capacity(args.len());
    for arg in args {
        quoted.push(quote_windows_argument(arg));
    }
    quoted.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_arguments() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(quote_windows_argument("a\"b"), "\"a\\\"b\"");
        assert_eq!(
            join_command_line(&["cmd", "two words"]),
            "cmd \"two words\""
        );
    }
}
