//! `gio mount` matching `gio/gio-tool-mount.c`.
//!
//! Mount, unmount, and list volumes. This port emits stub status lines
//! instead of driving a volume monitor main loop.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::File;
use crate::gio_tool::{
    print_file_error, print_line, register_tool_file_platform, reset_tool_state, show_help,
};
use crate::prelude::*;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
enum MountAction {
    Mount,
    Unmount,
    Eject,
    List,
    Monitor,
}

struct MountOptions {
    action: MountAction,
    list_detail: bool,
    device_id: Option<String>,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            action: MountAction::Mount,
            list_detail: false,
            device_id: None,
        }
    }
}

static STUB_MOUNTS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn parse_mount_args<'a>(args: &'a [&'a str]) -> Result<(MountOptions, Vec<&'a str>), &'static str> {
    let mut opts = MountOptions::default();
    let mut paths = Vec::new();
    for arg in args {
        match *arg {
            "--help" | "-h" => return Err("help"),
            "-u" | "--unmount" => opts.action = MountAction::Unmount,
            "-e" | "--eject" => opts.action = MountAction::Eject,
            "-l" | "--list" => opts.action = MountAction::List,
            "-o" | "--monitor" => opts.action = MountAction::Monitor,
            "-i" | "--detail" => opts.list_detail = true,
            "-d" | "--device" => {}
            other if other.starts_with("--device=") => {
                opts.device_id = Some(other.trim_start_matches("--device=").to_string());
            }
            other if other.starts_with('-') => {}
            other => paths.push(other),
        }
    }
    Ok((opts, paths))
}

fn list_mounts(detail: bool) {
    let mounts = STUB_MOUNTS.lock();
    if mounts.is_empty() {
        print_line("Drive(0): Local Drive");
        print_line("  Type: StubDrive");
        if detail {
            print_line("  is_removable=0");
        }
        return;
    }
    for (i, (name, uri)) in mounts.iter().enumerate() {
        print_line(&format!("Mount({i}): {name} -> {uri}"));
        if detail {
            print_line("  Type: StubMount");
        }
    }
}

fn mount_location(file: &File, action: MountAction) -> bool {
    let uri = file.get_uri();
    match action {
        MountAction::Mount => {
            STUB_MOUNTS.lock().push((
                file.get_basename().unwrap_or_else(|| "volume".to_string()),
                uri.clone(),
            ));
            print_line(&format!("mounted {uri}"));
            true
        }
        MountAction::Unmount => {
            STUB_MOUNTS.lock().retain(|(_, u)| u != &uri);
            print_line(&format!("unmounted {uri}"));
            true
        }
        MountAction::Eject => {
            print_line(&format!("ejected {uri}"));
            true
        }
        MountAction::List | MountAction::Monitor => true,
    }
}

/// Run `gio mount` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();
    STUB_MOUNTS.lock().clear();

    let (opts, paths) = match parse_mount_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help("mount", "Mount or unmount locations.", "[LOCATION…]", None);
            return 0;
        }
        Err(_) => return 1,
    };

    if let Some(id) = &opts.device_id {
        print_line(&format!("mount device {id} (stub)"));
        return 0;
    }

    match opts.action {
        MountAction::List => {
            list_mounts(opts.list_detail);
            return 0;
        }
        MountAction::Monitor => {
            print_line("Monitoring mount events. Press Ctrl+C to quit.");
            return 0;
        }
        _ => {}
    }

    if paths.is_empty() {
        show_help(
            "mount",
            "Mount or unmount locations.",
            "[LOCATION…]",
            Some("No locations given"),
        );
        return 1;
    }

    let mut ok = true;
    for path in paths {
        let file = File::new_for_commandline_arg(path);
        if !file.query_exists(None) && opts.action == MountAction::Mount {
            print_file_error(&file, "No such file or directory");
            ok = false;
            continue;
        }
        ok &= mount_location(&file, opts.action.clone());
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
    use crate::gio_tool::with_tool_vfs;

    #[test]
    fn test_mount_list_stub() {
        assert_eq!(run(&["--list"]), 0);
        assert!(core::str::from_utf8(&take_stdout())
            .unwrap()
            .contains("Drive"));
    }

    #[test]
    fn test_mount_and_unmount() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_dir("/media/usb");
        });
        assert_eq!(run(&["/media/usb"]), 0);
        assert_eq!(run(&["-u", "/media/usb"]), 0);
    }

    #[test]
    fn test_mount_no_paths() {
        assert_eq!(run(&[]), 1);
    }
}
