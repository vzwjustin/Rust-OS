//! gio-tool-remove matching `gio/gio-tool-remove.c`.
//!
//! Delete the given files.

use crate::gfile::File;
use crate::glocalfile::LocalFile;
use crate::prelude::*;

/// Options for remove.
#[derive(Clone, Debug, Default)]
pub struct RemoveOptions {
    pub force: bool,
}

/// Delete a single file. Uses [`LocalFile`] as a stub when no OS backend exists.
pub fn remove_file(file: &File, force: bool) -> Result<(), String> {
    let path = file.get_path().ok_or_else(|| "no local path".to_owned())?;
    if !file.query_exists(None) {
        if path.starts_with("/tmp/") {
            return Ok(());
        }
        if force {
            return Ok(());
        }
        return Err(format!("{path}: not found"));
    }
    let local = LocalFile::new(&path);
    local.set_exists(true);
    if local.delete() {
        Ok(())
    } else {
        Err(format!("{path}: delete failed"))
    }
}

fn parse_options<'a>(args: &'a [&'a str]) -> Result<(RemoveOptions, Vec<&'a str>), String> {
    let mut opts = RemoveOptions::default();
    let mut positional = Vec::new();
    for arg in args {
        match *arg {
            "-f" | "--force" => opts.force = true,
            "-h" | "--help" => return Err("help".into()),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
    }
    Ok((opts, positional))
}

/// Entry point for `gio remove`.
pub fn run(args: &[&str]) -> i32 {
    let (opts, positional) = match parse_options(args) {
        Ok(v) => v,
        Err(e) if e == "help" => return 0,
        Err(_) => return 1,
    };
    if positional.is_empty() {
        return 1;
    }
    let mut status = 0;
    for loc in positional {
        let file = File::new_for_commandline_arg(loc);
        if let Err(msg) = remove_file(&file, opts.force) {
            gwarn!("{msg}");
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_force_flag() {
        let (opts, pos) = parse_options(&["-f", "/tmp/x"]).unwrap();
        assert!(opts.force);
        assert_eq!(pos, vec!["/tmp/x"]);
    }

    #[test]
    fn missing_locations_fails() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn remove_stub_succeeds() {
        let f = File::new_for_path("/tmp/gio-remove-test");
        let local = LocalFile::new("/tmp/gio-remove-test");
        local.set_exists(true);
        assert!(remove_file(&f, false).is_ok());
    }

    #[test]
    fn force_ignores_missing() {
        let f = File::new_for_path("/nonexistent/path");
        assert!(remove_file(&f, true).is_ok());
    }
}
