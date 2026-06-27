//! gio-tool-trash matching `gio/gio-tool-trash.c`.
//!
//! Move files to the trash or manage trash contents.

use crate::gfile::File;
use crate::gtrashportal::TrashPortal;
use crate::prelude::*;

/// Trash operation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashMode {
    Trash,
    Restore,
    List,
    Empty,
}

impl Default for TrashMode {
    fn default() -> Self {
        Self::Trash
    }
}

/// Options for trash.
#[derive(Clone, Debug, Default)]
pub struct TrashOptions {
    pub force: bool,
    pub mode: TrashMode,
}

/// Move a file to trash via [`TrashPortal`] stub.
pub fn trash_file(path: &str, force: bool) -> Result<(), String> {
    let file = File::new_for_commandline_arg(path);
    if !file.query_exists(None) && !force {
        return Err(format!("{path}: not found"));
    }
    let portal = TrashPortal::new();
    portal.set_available(true);
    if portal.trash_file(path) {
        Ok(())
    } else {
        Err(format!("{path}: trash failed"))
    }
}

/// List trashed items (stub returns empty).
pub fn list_trash() -> Vec<(String, String)> {
    Vec::new()
}

/// Empty the trash (stub).
pub fn empty_trash() -> Result<(), String> {
    Ok(())
}

fn parse_options<'a>(args: &'a [&'a str]) -> Result<(TrashOptions, Vec<&'a str>), String> {
    let mut opts = TrashOptions {
        mode: TrashMode::Trash,
        ..Default::default()
    };
    let mut positional = Vec::new();
    for arg in args {
        match *arg {
            "-f" | "--force" => opts.force = true,
            "--empty" => opts.mode = TrashMode::Empty,
            "--list" => opts.mode = TrashMode::List,
            "--restore" => opts.mode = TrashMode::Restore,
            "-h" | "--help" => return Err("help".into()),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
    }
    Ok((opts, positional))
}

/// Entry point for `gio trash`.
pub fn run(args: &[&str]) -> i32 {
    let (opts, positional) = match parse_options(args) {
        Ok(v) => v,
        Err(e) if e == "help" => return 0,
        Err(_) => return 1,
    };
    match opts.mode {
        TrashMode::List if positional.is_empty() => {
            for (uri, orig) in list_trash() {
                gwarn!("{uri}\t{orig}");
            }
            0
        }
        TrashMode::Empty if positional.is_empty() => match empty_trash() {
            Ok(()) => 0,
            Err(msg) => {
                gwarn!("{msg}");
                1
            }
        },
        TrashMode::Trash | TrashMode::Restore => {
            if positional.is_empty() {
                return 1;
            }
            let mut status = 0;
            for loc in positional {
                if opts.mode == TrashMode::Restore && !loc.starts_with("trash:") {
                    gwarn!("{loc}: location must start with trash:///");
                    status = 1;
                    continue;
                }
                if let Err(msg) = trash_file(loc, opts.force) {
                    gwarn!("{msg}");
                    status = 1;
                }
            }
            status
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_flag() {
        let (opts, pos) = parse_options(&["--empty"]).unwrap();
        assert_eq!(opts.mode, TrashMode::Empty);
        assert!(pos.is_empty());
    }

    #[test]
    fn trash_stub_succeeds() {
        assert!(trash_file("/tmp/item", true).is_ok());
    }

    #[test]
    fn no_locations_without_mode_fails() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn list_mode_ok() {
        assert_eq!(run(&["--list"]), 0);
    }
}
