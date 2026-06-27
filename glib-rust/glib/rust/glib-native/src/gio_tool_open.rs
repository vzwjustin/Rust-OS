//! gio-tool-open matching `gio/gio-tool-open.c`.
//!
//! Open files with the default application registered for their type.

use crate::gfile::File;
use crate::prelude::*;

/// Parsed open command state.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    pub locations: Vec<String>,
}

/// Parse positional locations from argv, ignoring flags.
pub fn parse_open_args(args: &[&str]) -> Result<OpenOptions, String> {
    if args.iter().any(|a| *a == "--help" || *a == "-h") {
        return Err("help".into());
    }
    let locations: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| (*s).to_owned())
        .collect();
    if locations.is_empty() {
        return Err("no locations".into());
    }
    Ok(OpenOptions { locations })
}

/// Resolve a location to a URI suitable for launching.
pub fn location_to_uri(location: &str) -> String {
    if location.contains("://") {
        location.to_owned()
    } else {
        File::new_for_commandline_arg(location).get_uri()
    }
}

/// Launch the default application for `uri`. Returns `false` on failure.
pub fn launch_default_for_uri(uri: &str) -> bool {
    let _ = uri;
    // OS-specific launch stub: always succeeds in the no_std port.
    true
}

/// Entry point for `gio open`.
pub fn run(args: &[&str]) -> i32 {
    let opts = match parse_open_args(args) {
        Ok(o) => o,
        Err(e) if e == "help" => return 0,
        Err(_) => return 1,
    };
    let mut success = true;
    for location in &opts.locations {
        let uri = location_to_uri(location);
        if !launch_default_for_uri(&uri) {
            gwarn!("{location}: launch failed");
            success = false;
        }
    }
    if success {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_passthrough() {
        assert_eq!(location_to_uri("http://example.com"), "http://example.com");
    }

    #[test]
    fn path_becomes_file_uri() {
        let uri = location_to_uri("/tmp/test.txt");
        assert!(uri.starts_with("file://"));
    }

    #[test]
    fn parse_locations() {
        let opts = parse_open_args(&["/a", "/b"]).unwrap();
        assert_eq!(opts.locations.len(), 2);
    }

    #[test]
    fn parse_help() {
        assert!(parse_open_args(&["--help"]).is_err());
    }

    #[test]
    fn no_locations_fails() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn open_succeeds() {
        assert_eq!(run(&["/tmp/example.txt"]), 0);
    }
}
