//! `girepository_private` matching `girepository/girepository-private.h`.
//!
//! Private internal API for `GIRepository`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use alloc::string::String;
use alloc::vec::Vec;

/// Internal repository state (mirrors private repository internals).
#[derive(Debug, Default)]
pub struct RepositoryPrivate {
    pub search_paths: Vec<String>,
    pub library_paths: Vec<String>,
}

impl RepositoryPrivate {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rp = RepositoryPrivate::new();
        assert!(rp.search_paths.is_empty());
    }
}
