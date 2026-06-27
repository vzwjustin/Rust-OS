//! GLocalFileEnumerator matching `gio/glocalfileenumerator.h`.
//! Enumerates files in a local directory. In this no_std port we model
//! it with a list of child file names.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A local file enumerator (`GLocalFileEnumerator`).
pub struct LocalFileEnumerator {
    dir_path: Mutex<String>,
    children: Mutex<Vec<String>>,
    index: Mutex<usize>,
}

impl LocalFileEnumerator {
    pub fn new(dir_path: &str) -> Self {
        Self {
            dir_path: Mutex::new(dir_path.to_string()),
            children: Mutex::new(Vec::new()),
            index: Mutex::new(0),
        }
    }

    pub fn add_child(&self, name: &str) {
        self.children.lock().push(name.to_string());
    }

    pub fn next(&self) -> Option<String> {
        let mut idx = self.index.lock();
        let children = self.children.lock();
        if *idx >= children.len() {
            return None;
        }
        let name = children[*idx].clone();
        *idx += 1;
        Some(name)
    }

    pub fn close(&self) {
        *self.index.lock() = 0;
    }

    pub fn count(&self) -> usize {
        self.children.lock().len()
    }
    pub fn get_dir_path(&self) -> String {
        self.dir_path.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate() {
        let e = LocalFileEnumerator::new("/tmp");
        e.add_child("file1.txt");
        e.add_child("file2.txt");
        assert_eq!(e.next(), Some("file1.txt".to_string()));
        assert_eq!(e.next(), Some("file2.txt".to_string()));
        assert!(e.next().is_none());
    }
}
