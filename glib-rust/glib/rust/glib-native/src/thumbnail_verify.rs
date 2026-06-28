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
///
/// This mirrors `thumbnail_verify` from `gio/thumbnail-verify.c`.
/// A thumbnail is considered valid if:
/// 1. The thumbnail file exists and can be stat'd
/// 2. The thumbnail's embedded `Thumb::MTime` PNG tEXt chunk matches `source_mtime`
/// 3. If no embedded mtime is found, the thumbnail file's own mtime is used as fallback
pub fn thumbnail_verify(
    thumbnail_path: &str,
    source_path: &str,
    source_mtime: u64,
) -> ThumbnailVerifyResult {
    if thumbnail_path.is_empty() || source_path.is_empty() {
        return ThumbnailVerifyResult::NotFound;
    }

    if !is_thumbnail_path(thumbnail_path) {
        return ThumbnailVerifyResult::Invalid;
    }

    let thumbnail_data = match crate::stdio::read_file_bytes(thumbnail_path) {
        Some(data) => data,
        None => return ThumbnailVerifyResult::NotFound,
    };

    if thumbnail_data.is_empty() {
        return ThumbnailVerifyResult::Invalid;
    }

    if let Some(embedded_mtime) = extract_thumb_mtime(&thumbnail_data) {
        if embedded_mtime == source_mtime {
            return ThumbnailVerifyResult::Ok;
        } else {
            return ThumbnailVerifyResult::Outdated;
        }
    }

    if let Some(thumb_stat) = crate::stdio::stat(thumbnail_path) {
        if thumb_stat.st_mtime as u64 == source_mtime {
            return ThumbnailVerifyResult::Ok;
        } else {
            return ThumbnailVerifyResult::Outdated;
        }
    }

    ThumbnailVerifyResult::NotFound
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

/// Extract the `Thumb::MTime` value from a PNG file's tEXt chunks.
///
/// PNG format: 8-byte signature, then chunks of [length: u32 BE, type: 4 bytes, data, crc: u32 BE].
/// tEXt chunks contain a keyword and text string separated by a NUL byte.
/// Returns the parsed mtime as a u64, or None if not found.
fn extract_thumb_mtime(data: &[u8]) -> Option<u64> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        return None;
    }

    let mut offset = 8usize;
    while offset + 8 <= data.len() {
        let length = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        let chunk_type = &data[offset + 4..offset + 8];

        if chunk_type == b"tEXt" {
            let chunk_data_start = offset + 8;
            let chunk_data_end = chunk_data_start + length;
            if chunk_data_end > data.len() {
                break;
            }
            let chunk_data = &data[chunk_data_start..chunk_data_end];

            if let Some(nul_pos) = chunk_data.iter().position(|&b| b == 0) {
                let keyword = &chunk_data[..nul_pos];
                let text = &chunk_data[nul_pos + 1..];

                if keyword == b"Thumb::MTime" {
                    let text_str = core::str::from_utf8(text).ok()?;
                    return text_str.trim().parse::<u64>().ok();
                }
            }
        }

        offset += 8 + length + 4;
    }

    None
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
    fn test_verify_invalid_path() {
        assert_eq!(
            thumbnail_verify("/thumb.png", "/source.png", 1000),
            ThumbnailVerifyResult::Invalid
        );
    }

    #[test]
    fn test_verify_not_found() {
        assert_eq!(
            thumbnail_verify(".cache/thumbnails/normal/abc.png", "/source.png", 1000),
            ThumbnailVerifyResult::NotFound
        );
    }

    #[test]
    fn test_extract_thumb_mtime_from_png() {
        let mut png = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        ];
        let keyword = b"Thumb::MTime";
        let value = b"1234567890";
        let text_data: Vec<u8> = keyword
            .iter()
            .chain(core::iter::once(&0u8))
            .chain(value.iter())
            .copied()
            .collect();
        let length = text_data.len() as u32;
        png.extend_from_slice(&length.to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(&text_data);
        png.extend_from_slice(&[0u8; 4]);
        assert_eq!(extract_thumb_mtime(&png), Some(1234567890));
    }

    #[test]
    fn test_extract_thumb_mtime_no_png() {
        assert_eq!(extract_thumb_mtime(b"not a png"), None);
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
