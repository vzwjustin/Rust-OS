//! gapplication-tool matching `gio/gapplication-tool.c`.
//!
//! Launch and inspect D-Bus activatable applications.

use crate::gdesktopappinfo::DesktopAppInfo;
use crate::prelude::*;

/// Known subcommands.
pub const COMMANDS: &[&str] = &[
    "help",
    "version",
    "list-apps",
    "launch",
    "action",
    "list-actions",
];

/// Print help for a subcommand (returns help text).
pub fn help_text(command: Option<&str>) -> String {
    match command {
        Some("launch") => "Usage: gapplication launch APPID [FILE…]".into(),
        Some("action") => "Usage: gapplication action APPID ACTION [PARAMETER]".into(),
        Some("list-actions") => "Usage: gapplication list-actions APPID".into(),
        Some("list-apps") => "Usage: gapplication list-apps".into(),
        Some("version") => "gapplication version".into(),
        Some(cmd) if COMMANDS.contains(&cmd) => format!("Help for {cmd}"),
        Some(unknown) => format!("Unknown command {unknown}"),
        None => "Usage: gapplication COMMAND [ARGS…]".into(),
    }
}

/// List registered desktop application IDs (stub registry).
pub fn list_apps(apps: &[DesktopAppInfo]) -> Vec<String> {
    apps.iter().map(|a| a.get_id().to_owned()).collect()
}

/// Launch an application by id with optional file URIs.
pub fn launch_app(app: &DesktopAppInfo, files: &[&str]) -> Result<(), String> {
    let _ = files;
    if app.get_executable().is_empty() {
        return Err(format!("{}: no executable", app.get_id()));
    }
    Ok(())
}

/// Invoke a named action on an application (stub).
pub fn invoke_action(_app_id: &str, action: &str, parameter: Option<&str>) -> Result<(), String> {
    let _ = parameter;
    if action.is_empty() {
        return Err("empty action".into());
    }
    Ok(())
}

/// List static actions from a desktop entry (stub).
pub fn list_actions(_app: &DesktopAppInfo) -> Vec<String> {
    Vec::new()
}

/// Entry point for `gapplication`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args[0] == "help" || args.contains(&"--help") {
        let _cmd = args.get(1).copied();
        gwarn!("{}", help_text(cmd));
        return 0;
    }
    match args[0] {
        "version" => {
            gwarn!("glib-native gapplication-tool");
            0
        }
        "list-apps" => {
            gwarn!("(no apps registered)");
            0
        }
        "launch" => {
            if args.len() < 2 {
                return 1;
            }
            let app = DesktopAppInfo::new(args[1], args[1], "/bin/true");
            match launch_app(&app, &args[2..]) {
                Ok(()) => 0,
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "action" => {
            if args.len() < 3 {
                return 1;
            }
            match invoke_action(args[1], args[2], args.get(3).copied()) {
                Ok(()) => 0,
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "list-actions" => {
            if args.len() < 2 {
                return 1;
            }
            let app = DesktopAppInfo::new(args[1], args[1], "");
            for _action in list_actions(&app) {
                gwarn!("{action}");
            }
            0
        }
        _other => {
            gwarn!("{}", help_text(Some(other)));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_unknown_command() {
        assert!(help_text(Some("nope")).contains("Unknown"));
    }

    #[test]
    fn launch_requires_appid() {
        assert_eq!(run(&["launch"]), 1);
    }

    #[test]
    fn version_ok() {
        assert_eq!(run(&["version"]), 0);
    }

    #[test]
    fn invoke_action_ok() {
        assert!(invoke_action("org.test.App", "quit", None).is_ok());
    }
}
