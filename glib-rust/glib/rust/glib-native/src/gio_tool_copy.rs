//! `gio copy` matching `gio/gio-tool-copy.c`.
//!
//! Copy one or more files between locations using [`File`], [`InputStream`],
//! and [`OutputStream`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::{File, FileCreateFlags};
use crate::gio_tool::{
    file_get_child, file_is_dir, print_file_error, register_tool_file_platform, reset_tool_state,
    show_help, vfs_store_from_stream, with_tool_vfs,
};
use crate::prelude::*;

const STREAM_BUFFER_SIZE: usize = 256 * 1024;

struct CopyOptions {
    no_target_directory: bool,
    overwrite: bool,
}

fn parse_copy_args<'a>(args: &'a [&'a str]) -> Result<(CopyOptions, Vec<&'a str>), &'static str> {
    let mut opts = CopyOptions {
        no_target_directory: false,
        overwrite: true,
    };
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--help" | "-h" => return Err("help"),
            "-T" | "--no-target-directory" => opts.no_target_directory = true,
            "-i" | "--interactive" => opts.overwrite = false,
            other if other.starts_with('-') => {}
            other => paths.push(other),
        }
        i += 1;
    }
    Ok((opts, paths))
}

fn copy_one(source: &File, target: &File, overwrite: bool) -> bool {
    let in_stream = match source.read(None) {
        Ok(s) => s,
        Err(e) => {
            print_file_error(source, e.message());
            return false;
        }
    };

    let out_stream = if overwrite {
        match target.replace(None, false, FileCreateFlags::ReplaceDestination, None) {
            Ok(s) => s,
            Err(e) => {
                print_file_error(target, e.message());
                return false;
            }
        }
    } else {
        match target.create(FileCreateFlags::None, None) {
            Ok(s) => s,
            Err(e) => {
                print_file_error(target, e.message());
                return false;
            }
        }
    };

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut ok = true;
    loop {
        match in_stream.read(&mut buffer, None) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = out_stream.write(&buffer[..n], None) {
                    print_file_error(target, e.message());
                    ok = false;
                    break;
                }
            }
            Err(e) => {
                print_file_error(source, e.message());
                ok = false;
                break;
            }
        }
    }
    let _ = in_stream.close(None);
    let _ = out_stream.close(None);

    if ok {
        if let Some(path) = target.get_path() {
            vfs_store_from_stream(&path, &out_stream);
        }
    }
    ok
}

/// Run `gio copy` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    let (opts, paths) = match parse_copy_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help(
                "copy",
                "Copy one or more files from SOURCE to DESTINATION.",
                "SOURCE… DESTINATION",
                None,
            );
            return 0;
        }
        Err(_) => return 1,
    };

    if paths.len() < 2 {
        show_help(
            "copy",
            "Copy one or more files from SOURCE to DESTINATION.",
            "SOURCE… DESTINATION",
            None,
        );
        return 1;
    }

    let dest = File::new_for_commandline_arg(paths[paths.len() - 1]);
    let dest_is_dir = file_is_dir(&dest);
    if !dest_is_dir && paths.len() > 2 {
        show_help(
            "copy",
            "Copy one or more files from SOURCE to DESTINATION.",
            "SOURCE… DESTINATION",
            Some("Destination is not a directory"),
        );
        return 1;
    }
    if opts.no_target_directory && paths.len() > 2 {
        show_help(
            "copy",
            "Copy one or more files from SOURCE to DESTINATION.",
            "SOURCE… DESTINATION",
            None,
        );
        return 1;
    }

    let mut ok = true;
    for src_path in &paths[..paths.len() - 1] {
        let source = File::new_for_commandline_arg(src_path);
        let target = if dest_is_dir && !opts.no_target_directory {
            let name = source.get_basename().unwrap_or_else(|| "file".to_string());
            file_get_child(&dest, &name)
        } else {
            dest.clone()
        };
        ok &= copy_one(&source, &target, opts.overwrite);
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
    fn test_copy_file() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_file("/src.txt", b"payload");
            vfs.add_dir("/dest");
        });
        assert_eq!(run(&["/src.txt", "/dest"]), 0);
        with_tool_vfs(|vfs| {
            assert_eq!(
                vfs.files.get("/dest/src.txt").map(|v| v.as_slice()),
                Some(b"payload" as &[u8])
            );
        });
    }

    #[test]
    fn test_copy_requires_two_paths() {
        assert_eq!(run(&["/only"]), 1);
    }

    #[test]
    fn test_copy_help() {
        assert_eq!(run(&["--help"]), 0);
    }
}
