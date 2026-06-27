//! `gio mkdir` matching `gio/gio-tool-mkdir.c`.
//!
//! Create directories in the tool VFS using [`File`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::File;
use crate::gio_tool::{
    print_file_error, register_tool_file_platform, reset_tool_state, show_help, with_tool_vfs,
};
use crate::prelude::*;

struct MkdirOptions {
    parents: bool,
}

fn parse_mkdir_args<'a>(args: &'a [&'a str]) -> Result<(MkdirOptions, Vec<&'a str>), &'static str> {
    let mut opts = MkdirOptions { parents: false };
    let mut paths = Vec::new();
    for arg in args {
        match *arg {
            "--help" | "-h" => return Err("help"),
            "-p" | "--parent" => opts.parents = true,
            other if other.starts_with('-') => {}
            other => paths.push(other),
        }
    }
    Ok((opts, paths))
}

fn make_directory(file: &File, parents: bool) -> bool {
    let path = match file.get_path() {
        Some(p) => p,
        None => {
            print_file_error(file, "Location has no path");
            return false;
        }
    };
    with_tool_vfs(|vfs| {
        if vfs.files.contains_key(&path) {
            print_file_error(file, "File exists");
            return false;
        }
        if vfs.dirs.contains(&path) {
            return true;
        }
        if parents {
            if let Some(parent) = parent_path(&path) {
                if !vfs.dirs.contains(&parent) && !vfs.files.contains_key(&parent) {
                    vfs.add_dir(&parent);
                }
            }
        } else if let Some(parent) = parent_path(&path) {
            if !vfs.dirs.contains(&parent) {
                print_file_error(file, "No such file or directory");
                return false;
            }
        }
        vfs.add_dir(&path);
        true
    })
}

fn parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    trimmed.rfind('/').map(|idx| {
        if idx == 0 {
            "/".to_string()
        } else {
            trimmed[..idx].to_string()
        }
    })
}

/// Run `gio mkdir` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    let (opts, paths) = match parse_mkdir_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help("mkdir", "Create directories.", "LOCATION…", None);
            return 0;
        }
        Err(_) => return 1,
    };

    if paths.is_empty() {
        show_help(
            "mkdir",
            "Create directories.",
            "LOCATION…",
            Some("No locations given"),
        );
        return 1;
    }

    let mut ok = true;
    for path in paths {
        let file = File::new_for_commandline_arg(path);
        ok &= make_directory(&file, opts.parents);
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
    fn test_mkdir_single() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["/newdir"]), 0);
        assert!(with_tool_vfs(|vfs| vfs.is_dir("/newdir")));
    }

    #[test]
    fn test_mkdir_parents() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["-p", "/a/b/c"]), 0);
        assert!(with_tool_vfs(|vfs| vfs.is_dir("/a/b/c")));
    }

    #[test]
    fn test_mkdir_missing_parent() {
        with_tool_vfs(|vfs| vfs.reset());
        assert_eq!(run(&["/lonely/child"]), 1);
    }
}
