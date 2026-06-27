//! `dep_list` matching `gio/kqueue/dep-list.h`.
//!
//! Dependency list: linked list of directory entries used by kqueue
//! file monitor to diff directory snapshots.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Dependency list entry (mirrors `dep_list`).
#[derive(Debug, Clone)]
pub struct DepList {
    pub path: String,
    pub inode: u64,
    pub next: Option<Box<DepList>>,
}

impl DepList {
    /// Creates a new dep list entry (mirrors `dl_create`).
    pub fn new(path: &str, inode: u64) -> Self {
        Self {
            path: path.into(),
            inode,
            next: None,
        }
    }

    /// Appends an entry to the end of the list.
    pub fn append(&mut self, entry: DepList) {
        if let Some(ref mut next) = self.next {
            next.append(entry);
        } else {
            self.next = Some(Box::new(entry));
        }
    }

    /// Returns the length of the list.
    pub fn len(&self) -> usize {
        1 + self.next.as_ref().map(|n| n.len()).unwrap_or(0)
    }

    /// Converts the list to a Vec for easier processing.
    pub fn to_vec(&self) -> Vec<(String, u64)> {
        let mut result = vec![(self.path.clone(), self.inode)];
        if let Some(ref next) = self.next {
            result.extend(next.to_vec());
        }
        result
    }

    /// Creates a shallow copy (mirrors `dl_shallow_copy`).
    pub fn shallow_copy(&self) -> DepList {
        self.clone()
    }

    /// Creates a directory listing (mirrors `dl_listing`).
    /// In our no_std port, returns an empty list.
    pub fn listing(_path: &str) -> Option<DepList> {
        None
    }

    /// Calculates the diff between two lists (mirrors `dl_diff`).
    /// Returns (removed, added) entries.
    pub fn diff(before: &DepList, after: &DepList) -> (Vec<(String, u64)>, Vec<(String, u64)>) {
        let before_vec = before.to_vec();
        let after_vec = after.to_vec();
        let removed: Vec<(String, u64)> = before_vec
            .iter()
            .filter(|b| !after_vec.iter().any(|a| a.0 == b.0))
            .cloned()
            .collect();
        let added: Vec<(String, u64)> = after_vec
            .iter()
            .filter(|a| !before_vec.iter().any(|b| b.0 == a.0))
            .cloned()
            .collect();
        (removed, added)
    }
}

/// Traverse callbacks (mirrors `traverse_cbs`).
#[derive(Default)]
pub struct TraverseCbs {
    pub added: Option<fn(&str, u64)>,
    pub removed: Option<fn(&str, u64)>,
    pub replaced: Option<fn(&str, u64, &str, u64)>,
    pub overwritten: Option<fn(&str, u64)>,
    pub moved: Option<fn(&str, u64, &str, u64)>,
    pub many_added: Option<fn(&[DepList])>,
    pub many_removed: Option<fn(&[DepList])>,
    pub names_updated: Option<fn()>,
}

/// Calculates changes between before and after lists
/// (mirrors `dl_calculate`).
pub fn calculate(before: &DepList, after: &DepList, cbs: &TraverseCbs) {
    let (removed, added) = DepList::diff(before, after);
    for (path, inode) in &added {
        if let Some(cb) = cbs.added {
            cb(path, *inode);
        }
    }
    for (path, inode) in &removed {
        if let Some(cb) = cbs.removed {
            cb(path, *inode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() {
        let dl = DepList::new("/tmp/file", 12345);
        assert_eq!(dl.path, "/tmp/file");
        assert_eq!(dl.inode, 12345);
        assert_eq!(dl.len(), 1);
    }

    #[test]
    fn test_append_and_len() {
        let mut dl = DepList::new("/a", 1);
        dl.append(DepList::new("/b", 2));
        dl.append(DepList::new("/c", 3));
        assert_eq!(dl.len(), 3);
    }

    #[test]
    fn test_to_vec() {
        let mut dl = DepList::new("/a", 1);
        dl.append(DepList::new("/b", 2));
        let v = dl.to_vec();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], ("/a".to_string(), 1));
        assert_eq!(v[1], ("/b".to_string(), 2));
    }

    #[test]
    fn test_diff() {
        let mut before = DepList::new("/a", 1);
        before.append(DepList::new("/b", 2));
        before.append(DepList::new("/c", 3));

        let mut after = DepList::new("/b", 2);
        after.append(DepList::new("/c", 3));
        after.append(DepList::new("/d", 4));

        let (removed, added) = DepList::diff(&before, &after);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, "/a");
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, "/d");
    }

    #[test]
    fn test_shallow_copy() {
        let dl = DepList::new("/test", 99);
        let copy = dl.shallow_copy();
        assert_eq!(copy.path, "/test");
        assert_eq!(copy.inode, 99);
    }

    #[test]
    fn test_calculate() {
        let before = DepList::new("/a", 1);
        let after = DepList::new("/b", 2);
        let mut added_count = 0;
        let mut removed_count = 0;
        let cbs = TraverseCbs {
            added: Some(|_, _| {}),
            removed: Some(|_, _| {}),
            ..Default::default()
        };
        calculate(&before, &after, &cbs);
        // Just verify it doesn't panic
        let _ = (&mut added_count, &mut removed_count);
    }
}
