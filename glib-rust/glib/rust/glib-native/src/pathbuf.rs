//! Path buffer matching `gpathbuf.h` / `gpathbuf.c`.
//!
//! A builder for constructing file paths incrementally.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// A path buffer (`GPathBuf`).
///
/// Builds paths by pushing path segments. Handles joining with `/`
/// and separating filename from extension.
#[derive(Clone, Debug)]
pub struct PathBuf {
    segments: Vec<String>,
}

impl PathBuf {
    /// Create a new empty path buffer (`g_path_buf_new`).
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Create from an existing path (`g_path_buf_new_from_path`).
    pub fn from_path(path: &str) -> Self {
        let mut buf = Self::new();
        buf.push(path);
        buf
    }

    /// Push a path segment (`g_path_buf_push`).
    ///
    /// Splits the path on `/` and adds each segment.
    pub fn push(&mut self, path: &str) -> &mut Self {
        for part in path.split('/') {
            if !part.is_empty() {
                self.segments.push(part.to_owned());
            }
        }
        self
    }

    /// Pop the last segment (`g_path_buf_pop`).
    ///
    /// Returns `true` if a segment was removed.
    pub fn pop(&mut self) -> bool {
        self.segments.pop().is_some()
    }

    /// Set the filename (last component) (`g_path_buf_set_filename`).
    ///
    /// Replaces the last segment if it exists, or adds one.
    /// Returns `true` on success.
    pub fn set_filename(&mut self, filename: &str) -> bool {
        if filename.is_empty() {
            return false;
        }
        if self.segments.is_empty() {
            self.segments.push(filename.to_owned());
        } else {
            let last = self.segments.len() - 1;
            self.segments[last] = filename.to_owned();
        }
        true
    }

    /// Set the extension of the filename (`g_path_buf_set_extension`).
    ///
    /// Replaces or appends the extension on the last segment.
    /// Returns `true` on success.
    pub fn set_extension(&mut self, ext: &str) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        let last = self.segments.len() - 1;
        let filename = &mut self.segments[last];

        // Remove existing extension if any
        if let Some(dot_pos) = filename.rfind('.') {
            if dot_pos > 0 {
                filename.truncate(dot_pos);
            }
        }

        if !ext.is_empty() {
            // Ensure no leading dot in ext
            let ext = ext.strip_prefix('.').unwrap_or(ext);
            filename.push('.');
            filename.push_str(ext);
        }
        true
    }

    /// Convert to a path string (`g_path_buf_to_path`).
    pub fn to_path(&self) -> String {
        if self.segments.is_empty() {
            return String::new();
        }
        self.segments.join("/")
    }

    /// Clear the buffer (`g_path_buf_clear`).
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    /// Consume and return the path string (`g_path_buf_clear_to_path`).
    pub fn into_path(self) -> String {
        self.to_path()
    }

    /// Check equality (`g_path_buf_equal`).
    pub fn equal(&self, other: &Self) -> bool {
        self.segments == other.segments
    }

    /// Get the number of segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

impl Default for PathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for PathBuf {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for PathBuf {}

impl core::fmt::Display for PathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_path() {
        let mut pb = PathBuf::new();
        pb.push("foo").push("bar");
        assert_eq!(pb.to_path(), "foo/bar");
    }

    #[test]
    fn from_path() {
        let pb = PathBuf::from_path("a/b/c");
        assert_eq!(pb.to_path(), "a/b/c");
    }

    #[test]
    fn pop() {
        let mut pb = PathBuf::from_path("a/b/c");
        assert!(pb.pop());
        assert_eq!(pb.to_path(), "a/b");
        assert!(pb.pop());
        assert_eq!(pb.to_path(), "a");
        assert!(pb.pop());
        assert!(!pb.pop());
    }

    #[test]
    fn set_filename() {
        let mut pb = PathBuf::from_path("a/b");
        assert!(pb.set_filename("test.txt"));
        assert_eq!(pb.to_path(), "a/test.txt");
    }

    #[test]
    fn set_extension() {
        let mut pb = PathBuf::from_path("a/file.txt");
        assert!(pb.set_extension("log"));
        assert_eq!(pb.to_path(), "a/file.log");
    }

    #[test]
    fn set_extension_new() {
        let mut pb = PathBuf::from_path("a/file");
        assert!(pb.set_extension("txt"));
        assert_eq!(pb.to_path(), "a/file.txt");
    }

    #[test]
    fn set_extension_remove() {
        let mut pb = PathBuf::from_path("a/file.txt");
        assert!(pb.set_extension(""));
        assert_eq!(pb.to_path(), "a/file");
    }

    #[test]
    fn equal() {
        let a = PathBuf::from_path("foo/bar");
        let b = PathBuf::from_path("foo/bar");
        let c = PathBuf::from_path("foo/baz");
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn clear() {
        let mut pb = PathBuf::from_path("a/b/c");
        pb.clear();
        assert!(pb.is_empty());
        assert_eq!(pb.to_path(), "");
    }

    #[test]
    fn into_path() {
        let pb = PathBuf::from_path("x/y/z");
        assert_eq!(pb.into_path(), "x/y/z");
    }

    #[test]
    fn push_with_slashes() {
        let mut pb = PathBuf::new();
        pb.push("/usr/local/bin");
        assert_eq!(pb.to_path(), "usr/local/bin");
    }

    #[test]
    fn display() {
        let pb = PathBuf::from_path("hello/world");
        assert_eq!(format!("{}", pb), "hello/world");
    }
}
