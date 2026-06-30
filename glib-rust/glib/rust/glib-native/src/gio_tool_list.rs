//! `gio list` matching `gio/gio-tool-list.c`.
//!
//! List directory contents using [`File`], [`FileEnumerator`], and
//! [`FileInfo`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::{File, FileInfo};
use crate::gfileenumerator::FileEnumerator;
use crate::gio_tool::{
    file_type_to_string, print_file_error, print_line, register_tool_file_platform,
    reset_tool_state, show_help, with_tool_vfs,
};
use crate::prelude::*;

struct ListOptions {
    show_hidden: bool,
    show_long: bool,
    print_uris: bool,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            show_hidden: false,
            show_long: false,
            print_uris: false,
        }
    }
}

fn parse_list_args<'a>(args: &'a [&'a str]) -> Result<(ListOptions, Vec<&'a str>), &'static str> {
    let mut opts = ListOptions::default();
    let mut paths = Vec::new();
    for arg in args {
        match *arg {
            "--help" | "-h" => return Err("help"),
            "-l" | "--long" => opts.show_long = true,
            "--hidden" => opts.show_hidden = true,
            "-u" | "--print-uris" => opts.print_uris = true,
            other if other.starts_with('-') => {}
            other => paths.push(other),
        }
    }
    Ok((opts, paths))
}

fn show_listing(info: &FileInfo, parent: &File, opts: &ListOptions) {
    if let Some("true") = info.get_attribute_string("standard::is-hidden") {
        if !opts.show_hidden {
            return;
        }
    }
    let name = info.get_name();
    let label = if opts.print_uris {
        let child = if parent.get_path().map(|p| p.ends_with('/')).unwrap_or(false) {
            format!("{}{}", parent.get_path().unwrap(), name)
        } else {
            format!(
                "{}/{}",
                parent.get_path().unwrap_or_else(|| "/".to_string()),
                name
            )
        };
        File::new_for_path(&child).get_uri()
    } else {
        name.to_string()
    };
    if opts.show_long {
        print_line(&format!(
            "{}\t{}\t({})",
            label,
            info.get_size(),
            file_type_to_string(info.get_file_type())
        ));
    } else {
        print_line(&label);
    }
}

fn list_directory(file: &File, opts: &ListOptions) -> bool {
    let path = match file.get_path() {
        Some(p) => p,
        None => {
            print_file_error(file, "Location has no path");
            return false;
        }
    };
    let entries = with_tool_vfs(|vfs| vfs.list_children(&path));
    let enumerator = FileEnumerator::new(file.clone(), entries);
    let mut ok = true;
    loop {
        match enumerator.next_file(None) {
            Ok(Some(info)) => show_listing(&info, file, opts),
            Ok(None) => break,
            Err(e) => {
                print_file_error(file, e.message());
                ok = false;
                break;
            }
        }
    }
    if let Err(e) = enumerator.close(None) {
        print_file_error(file, e.message());
        ok = false;
    }
    ok
}

/// Run `gio list` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    let (opts, paths) = match parse_list_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help(
                "list",
                "List the contents of locations.",
                "[LOCATION…]",
                None,
            );
            return 0;
        }
        Err(_) => return 1,
    };

    let targets: Vec<File> = if paths.is_empty() {
        vec![File::new_for_path("/")]
    } else {
        paths
            .iter()
            .map(|p| File::new_for_commandline_arg(p))
            .collect()
    };

    let mut ok = true;
    for file in targets {
        ok &= list_directory(&file, &opts);
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

    #[test]
    fn test_list_directory() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_dir("/tmp");
            vfs.add_file("/tmp/a.txt", b"a");
            vfs.add_file("/tmp/b.txt", b"b");
        });
        assert_eq!(run(&["/tmp"]), 0);
        let out = take_stdout();
        let text = core::str::from_utf8(&out).unwrap();
        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
    }

    #[test]
    fn test_list_long() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_dir("/data");
            vfs.add_file("/data/x", b"xyz");
        });
        assert_eq!(run(&["-l", "/data"]), 0);
        assert!(core::str::from_utf8(&take_stdout())
            .unwrap()
            .contains("regular"));
    }

    #[test]
    fn test_list_default_cwd() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_file("/root.txt", b"");
        });
        assert_eq!(run(&[]), 0);
    }
}
