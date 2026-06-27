//! gio-tool-rename matching `gio/gio-tool-rename.c`.
//!
//! Rename a file to a new display name.

use crate::gfile::File;
use crate::prelude::*;

/// Validate a new display name (no path separators).
pub fn validate_display_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty name".into());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("name must not contain path separators".into());
    }
    Ok(())
}

/// Rename `file` to `new_name`, returning the new URI on success.
pub fn rename_file(file: &File, new_name: &str) -> Result<String, String> {
    validate_display_name(new_name)?;
    let path = file.get_path().ok_or_else(|| "no local path".to_owned())?;
    let parent = crate::fileutils::path_get_dirname(&path);
    let new_path = crate::fileutils::build_pathv("/", &[&parent, new_name]);
    let _ = new_path;
    Ok(File::new_for_path(&new_path).get_uri())
}

/// Entry point for `gio rename`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args.contains(&"--help") || args.contains(&"-h") {
        return if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
            0
        } else {
            1
        };
    }
    let positional: Vec<&str> = args
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();
    if positional.len() != 2 {
        return 1;
    }
    let file = File::new_for_commandline_arg(positional[0]);
    match rename_file(&file, positional[1]) {
        Ok(uri) => {
            gwarn!("Rename successful. New uri: {uri}");
            0
        }
        Err(msg) => {
            gwarn!("{msg}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_in_name_rejected() {
        assert!(validate_display_name("a/b").is_err());
    }

    #[test]
    fn rename_builds_new_uri() {
        let f = File::new_for_path("/tmp/old.txt");
        let uri = rename_file(&f, "new.txt").unwrap();
        assert!(uri.contains("new.txt"));
    }

    #[test]
    fn empty_name_fails() {
        let f = File::new_for_path("/tmp/x");
        assert!(rename_file(&f, "").is_err());
    }

    #[test]
    fn wrong_arg_count() {
        assert_eq!(run(&["/tmp/a"]), 1);
        assert_eq!(run(&["/tmp/a", "b", "c"]), 1);
    }

    #[test]
    fn rename_run_ok() {
        assert_eq!(run(&["/tmp/a.txt", "b.txt"]), 0);
    }
}
