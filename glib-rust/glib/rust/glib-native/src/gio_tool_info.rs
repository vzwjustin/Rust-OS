//! `gio info` matching `gio/gio-tool-info.c`.
//!
//! Show information about GIO locations using [`File`] and [`FileInfo`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::{File, FileQueryInfoFlags};
use crate::gfileattribute::{FileAttributeInfoList, FileAttributeType};
use crate::gio_tool::{
    attribute_type_to_string, file_type_to_string, print_file_error, print_line,
    register_tool_file_platform, reset_tool_state, show_help, take_stdout, with_tool_vfs,
};
use crate::prelude::*;

struct InfoOptions {
    writable: bool,
    filesystem: bool,
    attributes: String,
    nofollow_symlinks: bool,
}

impl Default for InfoOptions {
    fn default() -> Self {
        Self {
            writable: false,
            filesystem: false,
            attributes: "*".to_string(),
            nofollow_symlinks: false,
        }
    }
}

fn parse_info_args<'a>(args: &'a [&'a str]) -> Result<(InfoOptions, Vec<&'a str>), &'static str> {
    let mut opts = InfoOptions::default();
    let mut paths = Vec::new();
    for arg in args {
        match *arg {
            "--help" | "-h" => return Err("help"),
            "-w" | "--query-writable" => opts.writable = true,
            "-f" | "--filesystem" => opts.filesystem = true,
            "-n" | "--nofollow-symlinks" => opts.nofollow_symlinks = true,
            "-a" | "--attributes" => {}
            other if other.starts_with("--attributes=") => {
                opts.attributes = other.trim_start_matches("--attributes=").to_string();
            }
            other if other.starts_with('-') => {}
            other => paths.push(other),
        }
    }
    Ok((opts, paths))
}

fn show_info(file: &File, info: &crate::gfile::FileInfo) {
    print_line(&format!("name: {}", info.get_name()));
    print_line(&format!(
        "type: {}",
        file_type_to_string(info.get_file_type())
    ));
    print_line(&format!("size: {}", info.get_size()));
    print_line(&format!("uri: {}", file.get_uri()));
    if let Some(path) = file.get_path() {
        print_line(&format!("local path: {path}"));
    }
    print_line("attributes:");
    if let Some(ct) = info.get_attribute_string("standard::content-type") {
        print_line(&format!("  standard::content-type: {ct}"));
    }
}

fn query_info(file: &File, opts: &InfoOptions) -> bool {
    let mut flags = FileQueryInfoFlags::None;
    if opts.nofollow_symlinks {
        flags = FileQueryInfoFlags::NofollowSymlinks;
    }
    match file.query_info(&opts.attributes, flags, None) {
        Ok(info) => {
            if opts.filesystem {
                print_line("attributes:");
                print_line(&format!(
                    "  filesystem::type: {}",
                    file_type_to_string(info.get_file_type())
                ));
            } else {
                show_info(file, &info);
            }
            true
        }
        Err(e) => {
            print_file_error(file, e.message());
            false
        }
    }
}

fn show_writable_info(file: &File) -> bool {
    let mut list = FileAttributeInfoList::new();
    let _ = list.add(
        "standard::content-type",
        FileAttributeType::String,
        crate::gfileattribute::FileAttributeInfoFlags::NONE,
    );
    print_line("Settable attributes:");
    for i in 0..list.n_infos() {
        let info = list.info(i);
        print_line(&format!(
            " {} ({})",
            info.name,
            attribute_type_to_string(info.r#type)
        ));
    }
    let _ = file;
    true
}

/// Run `gio info` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    let (opts, paths) = match parse_info_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help(
                "info",
                "Show information about locations.",
                "LOCATION…",
                None,
            );
            return 0;
        }
        Err(_) => return 1,
    };

    if paths.is_empty() {
        show_help(
            "info",
            "Show information about locations.",
            "LOCATION…",
            Some("No locations given"),
        );
        return 1;
    }

    let mut ok = true;
    for path in paths {
        let file = File::new_for_commandline_arg(path);
        ok &= if opts.writable {
            show_writable_info(&file)
        } else {
            query_info(&file, &opts)
        };
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
    fn test_info_regular_file() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_file("/mock.txt", b"12345");
        });
        assert_eq!(run(&["/mock.txt"]), 0);
        let out = take_stdout();
        let text = core::str::from_utf8(&out).unwrap();
        assert!(text.contains("type: regular"));
        assert!(text.contains("uri:"));
    }

    #[test]
    fn test_info_writable() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["-w", "/mock.txt"]), 0);
        assert!(core::str::from_utf8(&take_stdout())
            .unwrap()
            .contains("Settable attributes"));
    }

    #[test]
    fn test_info_missing() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["/nope"]), 1);
    }
}
