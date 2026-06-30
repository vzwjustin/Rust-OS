//! `gio launch` matching `gio/gio-tool-launch.c`.
//!
//! Launch an application from a desktop file. This port records launch
//! requests in the tool stdout buffer rather than spawning processes.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::{AppInfo, AppLaunchContext};
use crate::gdesktopappinfo::DesktopAppInfo;
use crate::gfile::File;
use crate::gio_tool::{
    print_error, print_line, register_tool_file_platform, reset_tool_state, show_help,
};
use crate::prelude::*;
use alloc::collections::BTreeMap;
use spin::Mutex;

static DESKTOP_FILES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

fn launch_desktop(desktop_path: &str, file_args: &[&str]) -> bool {
    let contents = match with_desktop_contents(desktop_path) {
        Some(c) => c,
        None => {
            print_error(&format!("Unable to load '{desktop_path}': No such file"));
            return false;
        }
    };

    let desktop_id = desktop_path.rsplit('/').next().unwrap_or(desktop_path);
    let app = match DesktopAppInfo::from_key_file(desktop_id, &contents) {
        Some(info) => info,
        None => {
            print_error(&format!(
                "Unable to load application information for '{desktop_path}'"
            ));
            return false;
        }
    };

    let ctx = AppLaunchContext::new();
    let mut launched = Vec::new();
    for arg in file_args {
        launched.push(File::new_for_commandline_arg(arg).get_uri());
    }
    print_line(&format!(
        "launch {} ({}) files={}",
        app.get_id(),
        app.get_executable(),
        launched.join(",")
    ));
    let _ = ctx;
    true
}

fn with_desktop_contents(path: &str) -> Option<String> {
    DESKTOP_FILES.lock().get(path).cloned()
}

/// Register a desktop file body for tool tests.
pub fn register_desktop_file(path: &str, contents: &str) {
    DESKTOP_FILES
        .lock()
        .insert(path.to_string(), contents.to_string());
}

/// Run `gio launch` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        show_help(
            "launch",
            "Launch an application from a desktop file.",
            "DESKTOP-FILE [FILE-ARG…]",
            None,
        );
        return if args.is_empty() { 1 } else { 0 };
    }

    let desktop = args[0];
    if launch_desktop(desktop, &args[1..]) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_from_registered_desktop() {
        register_desktop_file(
            "/apps/test.desktop",
            "[Desktop Entry]\nName=Demo\nExec=/usr/bin/demo\n",
        );
        assert_eq!(run(&["/apps/test.desktop", "/doc.txt"]), 0);
        let out = take_stdout();
        assert!(core::str::from_utf8(&out).unwrap().contains("launch"));
    }

    #[test]
    fn test_launch_missing_desktop() {
        assert_eq!(run(&["/missing.desktop"]), 1);
    }

    #[test]
    fn test_launch_help() {
        assert_eq!(run(&["--help"]), 0);
    }
}
