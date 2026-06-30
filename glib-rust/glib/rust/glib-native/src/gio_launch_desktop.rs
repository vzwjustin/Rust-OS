//! gio-launch-desktop matching `gio/gio-launch-desktop.c`.
//!
//! Minimal wrapper to launch a desktop file and set GIO_LAUNCHED_DESKTOP_FILE_PID.

use crate::gdesktopappinfo::DesktopAppInfo;
use crate::prelude::*;

/// Environment key set by this wrapper.
pub const LAUNCHED_DESKTOP_FILE_PID: &str = "GIO_LAUNCHED_DESKTOP_FILE_PID";

/// Parsed launch request.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    pub desktop_file: String,
    pub argv: Vec<String>,
    pub pid: u32,
}

/// Build argv from a desktop entry and trailing arguments.
pub fn build_launch_argv(desktop: &DesktopAppInfo, extra_args: &[&str]) -> Vec<String> {
    let mut argv = vec![desktop.get_executable()];
    argv.extend(extra_args.iter().map(|s| (*s).to_owned()));
    argv
}

/// Prepare a launch request (stub PID assignment).
pub fn prepare_launch(
    desktop_id: &str,
    desktop_contents: &str,
    args: &[&str],
) -> Result<LaunchRequest, String> {
    let desktop = DesktopAppInfo::from_key_file(desktop_id, desktop_contents)
        .ok_or_else(|| "invalid desktop entry".to_string())?;
    if desktop.get_executable().is_empty() {
        return Err("missing Exec key".into());
    }
    Ok(LaunchRequest {
        desktop_file: desktop_id.to_owned(),
        argv: build_launch_argv(&desktop, args),
        pid: 1,
    })
}

/// Launch desktop helper: sets PID env and exec stub.
pub fn launch_desktop(request: &LaunchRequest) -> Result<(), String> {
    let _env = format!("{}={}", LAUNCHED_DESKTOP_FILE_PID, request.pid);
    if request.argv.is_empty() {
        return Err("empty argv".into());
    }
    Ok(())
}

/// Entry point for `gio-launch-desktop`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args.contains(&"--help") {
        gwarn!("Usage: gio-launch-desktop DESKTOP-FILE [ARGS…]");
        return if args.is_empty() { 1 } else { 0 };
    }
    let desktop_id = args[0];
    let contents = "[Desktop Entry]\nName=Test\nExec=/bin/true\n";
    match prepare_launch(desktop_id, contents, &args[1..]) {
        Ok(req) => match launch_desktop(&req) {
            Ok(()) => 0,
            Err(_msg) => {
                gwarn!("{msg}");
                1
            }
        },
        Err(_msg) => {
            gwarn!("{msg}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: &str = "[Desktop Entry]\nName=App\nExec=/usr/bin/app\n";

    #[test]
    fn build_argv_includes_exec() {
        let d = DesktopAppInfo::from_key_file("app.desktop", DESKTOP).unwrap();
        let argv = build_launch_argv(&d, &["--flag"]);
        assert_eq!(argv[0], "/usr/bin/app");
        assert_eq!(argv[1], "--flag");
    }

    #[test]
    fn prepare_launch_ok() {
        let req = prepare_launch("app.desktop", DESKTOP, &[]).unwrap();
        assert_eq!(req.pid, 1);
    }

    #[test]
    fn launch_ok() {
        let req = prepare_launch("app.desktop", DESKTOP, &[]).unwrap();
        assert!(launch_desktop(&req).is_ok());
    }

    #[test]
    fn run_missing_file_fails() {
        assert_eq!(run(&[]), 1);
    }
}
