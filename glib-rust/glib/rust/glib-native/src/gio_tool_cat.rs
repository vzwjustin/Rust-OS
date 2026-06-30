//! `gio cat` matching `gio/gio-tool-cat.c`.
//!
//! Concatenate files to the tool stdout buffer using [`File`] and
//! [`InputStream`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::File;
use crate::ginputstream::InputStream;
use crate::gio_tool::{
    append_stdout, print_file_error, register_tool_file_platform, reset_tool_state, show_help,
};

const STREAM_BUFFER_SIZE: usize = 256 * 1024;

fn cat(file: &File) -> bool {
    let stream: InputStream = match file.read(None) {
        Ok(s) => s,
        Err(e) => {
            print_file_error(file, e.message());
            return false;
        }
    };

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut success = true;
    loop {
        match stream.read(&mut buffer, None) {
            Ok(0) => break,
            Ok(n) => append_stdout(&buffer[..n]),
            Err(e) => {
                print_file_error(file, e.message());
                success = false;
                break;
            }
        }
    }
    if let Err(e) = stream.close(None) {
        print_file_error(file, e.message());
        success = false;
    }
    success
}

/// Run `gio cat` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        show_help(
            "cat",
            "Concatenate files and print to standard output.",
            "LOCATION…",
            None,
        );
        return if args.is_empty() { 1 } else { 0 };
    }

    let mut ok = true;
    let mut saw_path = false;
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        saw_path = true;
        let file = File::new_for_commandline_arg(arg);
        ok &= cat(&file);
    }

    if !saw_path {
        show_help(
            "cat",
            "Concatenate files and print to standard output.",
            "LOCATION…",
            Some("No locations given"),
        );
        return 1;
    }

    if ok {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gio_tool::{take_stderr, with_tool_vfs};

    #[test]
    fn test_cat_success() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_file("/hello.txt", b"hello world");
        });
        assert_eq!(run(&["/hello.txt"]), 0);
        assert_eq!(take_stdout(), b"hello world".to_vec());
    }

    #[test]
    fn test_cat_missing_file() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["/missing.txt"]), 1);
        assert!(!take_stderr().is_empty());
    }

    #[test]
    fn test_cat_no_args() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn test_cat_help() {
        assert_eq!(run(&["--help"]), 0);
    }
}
