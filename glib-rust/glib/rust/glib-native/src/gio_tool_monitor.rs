//! `gio monitor` matching `gio/gio-tool-monitor.c`.
//!
//! Monitor files and directories for changes. This port registers watches
//! and formats stub events instead of blocking in a main loop.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::{File, FileType};
use crate::gfilemonitor::{FileMonitor, FileMonitorEvent};
use crate::gio_tool::{
    print_file_error, print_line, register_tool_file_platform, reset_tool_state, show_help,
};
use crate::prelude::*;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchKind {
    Dir,
    File,
    Auto,
}

#[derive(Clone)]
struct WatchRequest {
    location: String,
    kind: WatchKind,
    silent: bool,
}

static MONITORS: Mutex<Vec<(WatchRequest, FileMonitor)>> = Mutex::new(Vec::new());

fn parse_monitor_args(args: &[&str]) -> Result<Vec<WatchRequest>, &'static str> {
    let mut watches = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--help" => return Err("help"),
            "-d" | "--dir" => {
                i += 1;
                if i < args.len() {
                    watches.push(WatchRequest {
                        location: args[i].to_string(),
                        kind: WatchKind::Dir,
                        silent: false,
                    });
                }
            }
            "-f" | "--file" => {
                i += 1;
                if i < args.len() {
                    watches.push(WatchRequest {
                        location: args[i].to_string(),
                        kind: WatchKind::File,
                        silent: false,
                    });
                }
            }
            "-s" | "--silent" => {
                i += 1;
                if i < args.len() {
                    watches.push(WatchRequest {
                        location: args[i].to_string(),
                        kind: WatchKind::File,
                        silent: true,
                    });
                }
            }
            other if other.starts_with('-') => {}
            other => watches.push(WatchRequest {
                location: other.to_string(),
                kind: WatchKind::Auto,
                silent: false,
            }),
        }
        i += 1;
    }
    Ok(watches)
}

fn add_watch(req: &WatchRequest) -> bool {
    let file = File::new_for_commandline_arg(&req.location);
    let kind = match req.kind {
        WatchKind::Dir => WatchKind::Dir,
        WatchKind::File => WatchKind::File,
        WatchKind::Auto => match file.query_info(
            "standard::type",
            crate::gfile::FileQueryInfoFlags::None,
            None,
        ) {
            Ok(info) if info.get_file_type() == FileType::Directory => WatchKind::Dir,
            Ok(_) => WatchKind::File,
            Err(e) => {
                print_file_error(&file, e.message());
                return false;
            }
        },
    };

    let monitor = FileMonitor::new();
    if !req.silent {
        let event = if kind == WatchKind::Dir {
            FileMonitorEvent::Created
        } else {
            FileMonitorEvent::Changed
        };
        monitor.emit_event(&req.location, None, event);
        let label = match event {
            FileMonitorEvent::Created => "created",
            FileMonitorEvent::Changed => "changed",
            _ => "event",
        };
        print_line(&format!("{}: {}: {}", req.location, req.location, label));
    }
    MONITORS.lock().push((req.clone(), monitor));
    true
}

/// Run `gio monitor` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    register_tool_file_platform();
    MONITORS.lock().clear();

    let watches = match parse_monitor_args(args) {
        Ok(v) => v,
        Err(e) if e == "help" => {
            show_help(
                "monitor",
                "Monitor files or directories for changes.",
                "LOCATION…",
                None,
            );
            return 0;
        }
        Err(_) => return 1,
    };

    if watches.is_empty() {
        show_help(
            "monitor",
            "Monitor files or directories for changes.",
            "LOCATION…",
            Some("No locations given"),
        );
        return 1;
    }

    let mut ok = true;
    for watch in &watches {
        ok &= add_watch(watch);
    }
    print_line("monitoring (stub): press Ctrl+C to quit");
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
    fn test_monitor_file() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_file("/watch.txt", b"x");
        });
        assert_eq!(run(&["/watch.txt"]), 0);
        assert!(!take_stdout().is_empty());
    }

    #[test]
    fn test_monitor_dir_flag() {
        with_tool_vfs(|vfs| {
            vfs.reset();
            vfs.add_dir("/watchdir");
        });
        assert_eq!(run(&["-d", "/watchdir"]), 0);
    }

    #[test]
    fn test_monitor_no_locations() {
        assert_eq!(run(&[]), 1);
    }
}
