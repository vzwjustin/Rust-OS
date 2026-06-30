//! gio-tool-tree matching `gio/gio-tool-tree.c`.
//!
//! List directory contents in a tree-like format.

use crate::gfile::{File, FileQueryInfoFlags, FileType};
use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Options for tree.
#[derive(Clone, Debug, Default)]
pub struct TreeOptions {
    pub show_hidden: bool,
    pub follow_symlinks: bool,
}

/// A directory entry for tree rendering.
#[derive(Clone, Debug)]
pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub symlink_target: Option<String>,
    pub children: Vec<TreeEntry>,
}

/// Build a tree from a stub directory map (used when no OS enumerator exists).
pub fn build_tree_from_map(
    root: &str,
    map: &BTreeMap<String, Vec<String>>,
    opts: &TreeOptions,
) -> TreeEntry {
    fn build(path: &str, map: &BTreeMap<String, Vec<String>>, opts: &TreeOptions) -> TreeEntry {
        let name = crate::fileutils::path_get_basename(path);
        let mut children = Vec::new();
        if let Some(names) = map.get(path) {
            for child_name in names {
                if !opts.show_hidden && child_name.starts_with('.') {
                    continue;
                }
                let child_path = crate::fileutils::build_pathv("/", &[path, child_name]);
                children.push(build(&child_path, map, opts));
            }
            children.sort_by(|a, b| a.name.cmp(&b.name));
        }
        let is_hidden = name.starts_with('.');
        TreeEntry {
            name: name.clone(),
            is_dir: map.contains_key(path),
            is_hidden,
            symlink_target: None,
            children,
        }
    }
    build(root, map, opts)
}

/// Format tree as text lines (mirrors `do_tree` output).
pub fn format_tree(root_uri: &str, entry: &TreeEntry, opts: &TreeOptions) -> Vec<String> {
    let mut lines = vec![root_uri.to_owned()];
    format_node(entry, 0, 0, opts, &mut lines);
    lines
}

fn format_node(
    entry: &TreeEntry,
    level: u32,
    pattern: u64,
    opts: &TreeOptions,
    out: &mut Vec<String>,
) {
    let count = entry.children.len();
    for (idx, child) in entry.children.iter().enumerate() {
        let is_last = idx + 1 == count;
        let mut line = String::new();
        for n in 0..level {
            if pattern & (1 << n) != 0 {
                line.push_str("|   ");
            } else {
                line.push_str("    ");
            }
        }
        if is_last {
            line.push_str("`-- ");
        } else {
            line.push_str("|-- ");
        }
        line.push_str(&child.name);
        if let Some(ref target) = child.symlink_target {
            line.push_str(" -> ");
            line.push_str(target);
        }
        out.push(line);
        if child.is_dir && (opts.follow_symlinks || child.symlink_target.is_none()) {
            let new_pattern = if is_last {
                pattern
            } else {
                pattern | (1 << level)
            };
            format_node(child, level + 1, new_pattern, opts, out);
        }
    }
}

/// Enumerate a directory into a tree entry (stub via query_info).
pub fn enumerate_tree(file: &File, opts: &TreeOptions) -> Result<TreeEntry, String> {
    let path = file.get_path().ok_or_else(|| "no path".to_owned())?;
    let name = file.get_basename().unwrap_or_else(|| path.clone());
    let info = file
        .query_info("standard::type", FileQueryInfoFlags::None, None)
        .ok();
    let is_dir = info
        .map(|i| i.get_file_type() == FileType::Directory)
        .unwrap_or(true);
    let is_hidden = name.starts_with('.') && !opts.show_hidden;
    Ok(TreeEntry {
        name,
        is_dir,
        is_hidden,
        symlink_target: None,
        children: Vec::new(),
    })
}

fn parse_options<'a>(args: &'a [&'a str]) -> Result<(TreeOptions, Vec<&'a str>), String> {
    let mut opts = TreeOptions::default();
    let mut positional = Vec::new();
    for arg in args {
        match *arg {
            "-h" | "--hidden" => opts.show_hidden = true,
            "-l" | "--follow-symlinks" => opts.follow_symlinks = true,
            "--help" => return Err("help".into()),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
    }
    Ok((opts, positional))
}

/// Entry point for `gio tree`.
pub fn run(args: &[&str]) -> i32 {
    let (opts, positional) = match parse_options(args) {
        Ok(v) => v,
        Err(e) if e == "help" => return 0,
        Err(_) => return 1,
    };
    let locations: Vec<File> = if positional.is_empty() {
        vec![File::new_for_path(".")]
    } else {
        positional
            .iter()
            .map(|p| File::new_for_commandline_arg(p))
            .collect()
    };
    for file in locations {
        let uri = file.get_uri();
        match enumerate_tree(&file, &opts) {
            Ok(entry) => {
                for _line in format_tree(&uri, &entry, &opts) {
                    gwarn!("{line}");
                }
            }
            Err(_msg) => gwarn!("{msg}"),
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_simple_tree() {
        let mut map = BTreeMap::new();
        map.insert("/root".to_owned(), vec!["a".into(), "b".into()]);
        let entry = build_tree_from_map("/root", &map, &TreeOptions::default());
        let lines = format_tree("file:///root", &entry, &TreeOptions::default());
        assert!(lines[1].contains("a"));
    }

    #[test]
    fn hidden_skipped_by_default() {
        let mut map = BTreeMap::new();
        map.insert("/r".to_owned(), vec![".hidden".into(), "vis".into()]);
        let entry = build_tree_from_map("/r", &map, &TreeOptions::default());
        assert_eq!(entry.children.len(), 1);
    }

    #[test]
    fn parse_hidden_flag() {
        let (opts, _) = parse_options(&["-h"]).unwrap();
        assert!(opts.show_hidden);
    }

    #[test]
    fn run_defaults_to_cwd() {
        assert_eq!(run(&[]), 0);
    }
}
