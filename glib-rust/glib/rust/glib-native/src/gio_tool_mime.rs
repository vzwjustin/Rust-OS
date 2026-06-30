//! `gio mime` matching `gio/gio-tool-mime.c`.
//!
//! Get or set the handler for a MIME type using an in-memory app registry.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::SimpleAppInfo;
use crate::gio_tool::{print_error, print_line, reset_tool_state, show_help};
use crate::prelude::*;
use alloc::collections::BTreeMap;
use spin::Mutex;

static MIME_DEFAULTS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static MIME_APPS: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());
static APP_BY_ID: Mutex<BTreeMap<String, SimpleAppInfo>> = Mutex::new(BTreeMap::new());

fn ensure_sample_apps() {
    let mut apps = APP_BY_ID.lock();
    if !apps.is_empty() {
        return;
    }
    apps.insert(
        "org.test.TextEditor".to_string(),
        SimpleAppInfo::new("org.test.TextEditor", "Text Editor", "/usr/bin/editor"),
    );
    apps.insert(
        "org.test.ImageViewer".to_string(),
        SimpleAppInfo::new("org.test.ImageViewer", "Image Viewer", "/usr/bin/viewer"),
    );
    MIME_APPS.lock().insert(
        "text/plain".to_string(),
        vec![
            "org.test.TextEditor".to_string(),
            "org.test.ImageViewer".to_string(),
        ],
    );
    MIME_DEFAULTS
        .lock()
        .insert("text/plain".to_string(), "org.test.TextEditor".to_string());
}

fn show_mime_handlers(mimetype: &str) {
    ensure_sample_apps();
    if let Some(default_id) = MIME_DEFAULTS.lock().get(mimetype) {
        print_line(&format!(
            "Default application for \"{mimetype}\": {default_id}"
        ));
    } else {
        print_line(&format!("No default applications for \"{mimetype}\""));
    }
    if let Some(ids) = MIME_APPS.lock().get(mimetype) {
        if ids.is_empty() {
            print_line("No registered applications");
        } else {
            print_line("Registered applications:");
            for id in ids {
                print_line(&format!("\t{id}"));
            }
        }
    } else {
        print_line("No registered applications");
    }
    print_line("No recommended applications");
}

fn set_mime_handler(mimetype: &str, handler: &str) -> bool {
    ensure_sample_apps();
    if !APP_BY_ID.lock().contains_key(handler) {
        print_error(&format!("Failed to load info for handler \"{handler}\""));
        return false;
    }
    MIME_DEFAULTS
        .lock()
        .insert(mimetype.to_string(), handler.to_string());
    print_line(&format!("Set {handler} as the default for {mimetype}"));
    true
}

/// Run `gio mime` with in-memory arguments.
pub fn run(args: &[&str]) -> i32 {
    reset_tool_state();
    ensure_sample_apps();

    if args.is_empty() || args[0] == "--help" {
        show_help(
            "mime",
            "Get or set the handler for a mimetype.",
            "MIMETYPE [HANDLER]",
            None,
        );
        return if args.is_empty() { 1 } else { 0 };
    }

    match args.len() {
        1 => {
            show_mime_handlers(args[0]);
            0
        }
        2 => {
            if set_mime_handler(args[0], args[1]) {
                0
            } else {
                1
            }
        }
        _ => {
            show_help(
                "mime",
                "Get or set the handler for a mimetype.",
                "MIMETYPE [HANDLER]",
                Some("Must specify a single mimetype, and maybe a handler"),
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_get_default() {
        assert_eq!(run(&["text/plain"]), 0);
        let out = take_stdout();
        assert!(core::str::from_utf8(&out)
            .unwrap()
            .contains("org.test.TextEditor"));
    }

    #[test]
    fn test_mime_set_handler() {
        assert_eq!(run(&["text/plain", "org.test.ImageViewer"]), 0);
        assert_eq!(
            MIME_DEFAULTS.lock().get("text/plain").map(String::as_str),
            Some("org.test.ImageViewer")
        );
    }

    #[test]
    fn test_mime_invalid_handler() {
        assert_eq!(run(&["text/plain", "missing.app"]), 1);
    }
}
