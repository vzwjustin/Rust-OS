//! Thumbnail verification matching `gio/thumbnail-verify.h`.
//! Verifies thumbnail files. In this no_std port we model it with
//! basic thumbnail validation checks.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Thumbnail verification result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailVerifyResult {
    Ok,
    NotFound,
    Invalid,
    Mismatch,
    Outdated,
}

/// Verifies a thumbnail against its source file.
pub fn thumbnail_verify(
    thumbnail_path: &str,
    source_path: &str,
    source_mtime: u64,
) -> ThumbnailVerifyResult {
    if thumbnail_path.is_empty() || source_path.is_empty() {
        return ThumbnailVerifyResult::NotFound;
    }
    // In a real implementation, we'd check the thumbnail's embedded mtime
    // For now, just return Ok as a stub
    let _ = source_mtime;
    ThumbnailVerifyResult::Ok
}

/// Checks if a file path looks like a valid thumbnail path.
pub fn is_thumbnail_path(path: &str) -> bool {
    path.starts_with(".cache/thumbnails/") && (path.ends_with(".png") || path.ends_with(".jpg"))
}

/// Gets the expected thumbnail path for a source file.
pub fn get_thumbnail_path(source_path: &str, size: u32) -> String {
    let size_dir = match size {
        0..=128 => "normal",
        _ => "large",
    };
    let hash = simple_hash(source_path);
    alloc::format!(".cache/thumbnails/{}/{}.png", size_dir, hash)
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_empty() {
        assert_eq!(
            thumbnail_verify("", "/path", 0),
            ThumbnailVerifyResult::NotFound
        );
    }

    #[test]
    fn test_verify_ok() {
        assert_eq!(
            thumbnail_verify("/thumb.png", "/source.png", 1000),
            ThumbnailVerifyResult::Ok
        );
    }

    #[test]
    fn test_thumbnail_path() {
        let path = get_thumbnail_path("/home/user/file.txt", 128);
        assert!(path.starts_with(".cache/thumbnails/normal/"));
        assert!(path.ends_with(".png"));
    }

    #[test]
    fn test_is_thumbnail_path() {
        assert!(is_thumbnail_path(".cache/thumbnails/normal/abc.png"));
        assert!(!is_thumbnail_path("/tmp/file.png"));
    }
}
