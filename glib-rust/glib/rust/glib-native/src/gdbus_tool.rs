//! gdbus-tool matching `gio/gdbus-tool.c`.
//!
//! Command-line utility for D-Bus introspection, monitoring, and method calls.

use crate::gdbusconnection::DBusConnection;
use crate::prelude::*;

/// gdbus subcommands.
pub const COMMANDS: &[&str] = &[
    "help",
    "introspect",
    "monitor",
    "call",
    "emit",
    "get-property",
    "set-property",
];

/// Print usage summary.
pub fn usage_text() -> &'static str {
    "Commands: help, introspect, monitor, call, emit, get-property, set-property"
}

/// Introspect a remote object (stub returns minimal XML).
pub fn introspect_xml(bus_name: &str, object_path: &str) -> String {
    format!(
        "<node><interface name=\"{bus_name}\"><method name=\"Ping\"/></interface><!-- {object_path} --></node>"
    )
}

/// Call a D-Bus method on a connection (stub).
pub fn call_method(
    conn: &DBusConnection,
    bus_name: &str,
    object_path: &str,
    interface: &str,
    method: &str,
) -> Result<String, String> {
    let _ = (conn, bus_name, object_path, interface);
    if method.is_empty() {
        return Err("empty method".into());
    }
    Ok(String::new())
}

/// Emit a D-Bus signal (stub).
pub fn emit_signal(
    conn: &DBusConnection,
    object_path: &str,
    interface: &str,
    signal: &str,
) -> Result<(), String> {
    let _ = (conn, object_path, interface);
    if signal.is_empty() {
        return Err("empty signal".into());
    }
    Ok(())
}

fn open_session_bus() -> Result<DBusConnection, String> {
    DBusConnection::new_for_address_sync("loopback:").map_err(|e| e.message().to_owned())
}

/// Entry point for `gdbus`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args[0] == "help" || args.contains(&"--help") {
        gwarn!("{}", usage_text());
        return if args.is_empty() { 1 } else { 0 };
    }
    let conn = match open_session_bus() {
        Ok(c) => c,
        Err(_msg) => {
            gwarn!("{msg}");
            return 1;
        }
    };
    match args[0] {
        "introspect" => {
            if args.len() < 3 {
                return 1;
            }
            gwarn!("{}", introspect_xml(args[1], args[2]));
            0
        }
        "call" => {
            if args.len() < 5 {
                return 1;
            }
            match call_method(&conn, args[1], args[2], args[3], args[4]) {
                Ok(reply) => {
                    if !reply.is_empty() {
                        gwarn!("{reply}");
                    }
                    0
                }
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "emit" => {
            if args.len() < 4 {
                return 1;
            }
            match emit_signal(&conn, args[1], args[2], args[3]) {
                Ok(()) => 0,
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "monitor" => {
            gwarn!("monitoring (stub)");
            0
        }
        other if COMMANDS.contains(&other) => 1,
        _other => {
            gwarn!("unknown command {other}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introspect_contains_interface() {
        let xml = introspect_xml("org.test", "/org/test");
        assert!(xml.contains("interface"));
    }

    #[test]
    fn call_empty_method_fails() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        assert!(call_method(&conn, "a", "/a", "i", "").is_err());
    }

    #[test]
    fn emit_ok() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        assert!(emit_signal(&conn, "/p", "i", "Changed").is_ok());
    }

    #[test]
    fn run_help_ok() {
        assert_eq!(run(&["help"]), 0);
    }
}
