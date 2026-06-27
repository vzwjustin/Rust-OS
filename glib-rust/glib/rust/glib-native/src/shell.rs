//! Shell quoting utilities matching `gshell.h` / `gshell.c`.
//!
//! Provides shell-style quoting/unquoting and command-line parsing.
//! Fully `no_std` compatible.

use crate::prelude::*;

/// Shell error codes (`GShellError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShellError {
    /// Mismatched or otherwise mangled quoting.
    BadQuoting,
    /// String to be parsed was empty.
    EmptyString,
    /// General failure.
    Failed,
}

/// Error domain quark for shell errors.
pub fn shell_error_quark() -> u32 {
    // Static quark for shell errors
    0x7368656C // "shel" in ASCII
}

/// Quote a string for shell use (`g_shell_quote`).
///
/// Wraps the string in single quotes, escaping any embedded single quotes
/// as `'\''`.
pub fn shell_quote(unquoted: &str) -> String {
    let mut result = String::with_capacity(unquoted.len() + 2);
    result.push('\'');
    for c in unquoted.chars() {
        if c == '\'' {
            result.push_str("'\\''");
        } else {
            result.push(c);
        }
    }
    result.push('\'');
    result
}

/// Unquote a shell-quoted string (`g_shell_unquote`).
///
/// Handles single quotes, double quotes, and backslash escapes.
pub fn shell_unquote(quoted: &str) -> Result<String, ShellError> {
    let s = quoted.trim();
    if s.is_empty() {
        return Err(ShellError::EmptyString);
    }

    let bytes = s.as_bytes();
    let mut buf: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                // Single-quoted: no escapes inside, until next '
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                buf.extend_from_slice(&bytes[start..i]);
                i += 1; // skip closing '
            }
            b'"' => {
                // Double-quoted: backslash escapes for $ ` " \ newline
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        match bytes[i + 1] {
                            b'$' | b'`' | b'"' | b'\\' | b'\n' => {
                                buf.push(bytes[i + 1]);
                                i += 2;
                            }
                            _ => {
                                buf.push(b'\\');
                                i += 1;
                            }
                        }
                    } else {
                        buf.push(bytes[i]);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                i += 1; // skip closing "
            }
            b'\\' => {
                // Backslash escape outside quotes
                if i + 1 >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                buf.push(bytes[i + 1]);
                i += 2;
            }
            _ => {
                buf.push(bytes[i]);
                i += 1;
            }
        }
    }

    String::from_utf8(buf).map_err(|_| ShellError::Failed)
}

/// Parse a command line into arguments (`g_shell_parse_argv`).
///
/// Splits on whitespace, respecting quotes and backslash escapes.
pub fn shell_parse_argv(command_line: &str) -> Result<Vec<String>, ShellError> {
    let s = command_line.trim();
    if s.is_empty() {
        return Err(ShellError::EmptyString);
    }

    let bytes = s.as_bytes();
    let mut args: Vec<String> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut i = 0;
    let mut in_arg = false;

    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                if in_arg {
                    args.push(
                        String::from_utf8(core::mem::take(&mut buf))
                            .map_err(|_| ShellError::Failed)?,
                    );
                    in_arg = false;
                }
                i += 1;
            }
            b'\'' => {
                in_arg = true;
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    buf.push(bytes[i]);
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                i += 1;
            }
            b'"' => {
                in_arg = true;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        match bytes[i + 1] {
                            b'$' | b'`' | b'"' | b'\\' | b'\n' => {
                                buf.push(bytes[i + 1]);
                                i += 2;
                            }
                            _ => {
                                buf.push(b'\\');
                                i += 1;
                            }
                        }
                    } else {
                        buf.push(bytes[i]);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                i += 1;
            }
            b'\\' => {
                in_arg = true;
                if i + 1 >= bytes.len() {
                    return Err(ShellError::BadQuoting);
                }
                buf.push(bytes[i + 1]);
                i += 2;
            }
            _ => {
                in_arg = true;
                buf.push(bytes[i]);
                i += 1;
            }
        }
    }

    if in_arg {
        args.push(String::from_utf8(buf).map_err(|_| ShellError::Failed)?);
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_simple() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn quote_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn unquote_single() {
        assert_eq!(shell_unquote("'hello'").unwrap(), "hello");
    }

    #[test]
    fn unquote_double() {
        assert_eq!(shell_unquote("\"hello world\"").unwrap(), "hello world");
    }

    #[test]
    fn unquote_escape() {
        assert_eq!(shell_unquote("hello\\ world").unwrap(), "hello world");
    }

    #[test]
    fn unquote_double_with_escape() {
        assert_eq!(
            shell_unquote("\"hello \\\"world\\\"\"").unwrap(),
            "hello \"world\""
        );
    }

    #[test]
    fn unquote_empty() {
        assert_eq!(shell_unquote("").unwrap_err(), ShellError::EmptyString);
    }

    #[test]
    fn unquote_unmatched() {
        assert_eq!(shell_unquote("'hello").unwrap_err(), ShellError::BadQuoting);
        assert_eq!(
            shell_unquote("\"hello").unwrap_err(),
            ShellError::BadQuoting
        );
    }

    #[test]
    fn parse_argv_simple() {
        let args = shell_parse_argv("hello world").unwrap();
        assert_eq!(args, vec!["hello", "world"]);
    }

    #[test]
    fn parse_argv_quoted() {
        let args = shell_parse_argv("'hello world' test").unwrap();
        assert_eq!(args, vec!["hello world", "test"]);
    }

    #[test]
    fn parse_argv_double_quoted() {
        let args = shell_parse_argv("\"hello world\" test").unwrap();
        assert_eq!(args, vec!["hello world", "test"]);
    }

    #[test]
    fn parse_argv_escaped() {
        let args = shell_parse_argv("hello\\ world test").unwrap();
        assert_eq!(args, vec!["hello world", "test"]);
    }

    #[test]
    fn parse_argv_empty() {
        assert_eq!(shell_parse_argv("").unwrap_err(), ShellError::EmptyString);
    }

    #[test]
    fn parse_argv_multiple_spaces() {
        let args = shell_parse_argv("  hello   world  ").unwrap();
        assert_eq!(args, vec!["hello", "world"]);
    }
}
