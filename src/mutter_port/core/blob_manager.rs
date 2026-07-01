//! MetaBlobManager ported from GNOME Mutter's src/core/meta-blob-manager.c
//!
//! MetaBlobManager manages DRM blob resources: it creates and caches DRM
//! property blobs (e.g. for gamma LUTs, color management matrices, and
//! mode sets). Each blob is a chunk of data that the DRM driver stores
//! and references by a uint32 handle.
//!
//! In Mutter this wraps the DRM_IOCTL_MODE_CREATEPROPBLOB and
//! DRM_IOCTL_MODE_DESTROYPROPBLOB ioctls. In the kernel, we can call
//! the DRM subsystem directly, but the blob manager provides the caching
//! and deduplication layer.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-blob-manager.c

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// A DRM blob: data + its kernel-assigned handle. Mirrors the
/// drm_mode_create_blob structure.
#[derive(Debug, Clone)]
pub struct Blob {
    /// Kernel-assigned blob handle (uint32 in DRM).
    pub handle: u32,
    /// The blob data.
    pub data: Vec<u8>,
    /// Whether the blob has been destroyed (freed in the DRM driver).
    pub destroyed: bool,
}

impl Blob {
    pub fn new(handle: u32, data: Vec<u8>) -> Self {
        Blob {
            handle,
            data,
            destroyed: false,
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// The blob manager. Mirrors MetaBlobManager.
///
/// Caches blobs by data content to avoid creating duplicate DRM blobs
/// for identical data (e.g. when setting the same gamma LUT on multiple
/// CRTCs). Also tracks blob handles for cleanup on shutdown.
#[derive(Debug)]
pub struct MetaBlobManager {
    /// All managed blobs, keyed by handle.
    blobs: BTreeMap<u32, Blob>,
    /// Content hash → handle mapping for deduplication.
    /// Uses a simple FNV-1a hash of the blob data.
    dedup: BTreeMap<u64, u32>,
    /// Next synthetic handle (for testing without a real DRM driver).
    next_handle: u32,
    /// Whether to use real DRM ioctls (false in unit tests).
    use_drm: bool,
}

impl MetaBlobManager {
    /// Create a new blob manager. Mirrors meta_blob_manager_new().
    pub fn new() -> Self {
        MetaBlobManager {
            blobs: BTreeMap::new(),
            dedup: BTreeMap::new(),
            next_handle: 1,
            use_drm: false,
        }
    }

    /// Create a blob manager that uses real DRM ioctls.
    pub fn new_with_drm() -> Self {
        let mut mgr = Self::new();
        mgr.use_drm = true;
        mgr
    }

    /// Create a DRM blob from data. Mirrors
    /// meta_blob_manager_acquire_blob().
    ///
    /// If a blob with identical data already exists, returns the existing
    /// handle (deduplication). Otherwise, creates a new blob.
    pub fn acquire(&mut self, data: &[u8]) -> u32 {
        let hash = fnv1a_hash(data);

        // Check for existing blob with same data.
        if let Some(&handle) = self.dedup.get(&hash) {
            if let Some(blob) = self.blobs.get(&handle) {
                if !blob.is_destroyed() && blob.data == data {
                    return handle;
                }
            }
            // Stale dedup entry; remove it.
            self.dedup.remove(&hash);
        }

        // Create new blob.
        let handle = if self.use_drm {
            // In real DRM mode, this would call:
            // drmIoctl(fd, DRM_IOCTL_MODE_CREATEPROPBLOB, &blob_create)
            // For now, use synthetic handles even in DRM mode.
            self.next_handle
        } else {
            self.next_handle
        };

        self.next_handle += 1;

        let blob = Blob::new(handle, data.to_vec());
        self.blobs.insert(handle, blob);
        self.dedup.insert(hash, handle);

        handle
    }

    /// Destroy a blob by handle. Mirrors
    /// meta_blob_manager_release_blob().
    ///
    /// Marks the blob as destroyed and removes it from the cache.
    /// In DRM mode, this would call DRM_IOCTL_MODE_DESTROYPROPBLOB.
    pub fn release(&mut self, handle: u32) -> bool {
        if let Some(blob) = self.blobs.get_mut(&handle) {
            if blob.destroyed {
                return false; // Already destroyed.
            }
            blob.destroyed = true;

            // Remove from dedup cache.
            let hash = fnv1a_hash(&blob.data);
            if self.dedup.get(&hash) == Some(&handle) {
                self.dedup.remove(&hash);
            }

            // Remove from blob map.
            self.blobs.remove(&handle);
            return true;
        }
        false
    }

    /// Get a blob by handle.
    pub fn get(&self, handle: u32) -> Option<&Blob> {
        self.blobs.get(&handle)
    }

    /// Number of active (non-destroyed) blobs.
    pub fn count(&self) -> usize {
        self.blobs.values().filter(|b| !b.destroyed).count()
    }

    /// Destroy all blobs. Mirrors meta_blob_manager_free().
    pub fn destroy_all(&mut self) {
        let handles: Vec<u32> = self.blobs.keys().copied().collect();
        for handle in handles {
            self.release(handle);
        }
    }

    /// All active blob handles.
    pub fn handles(&self) -> Vec<u32> {
        self.blobs
            .values()
            .filter(|b| !b.destroyed)
            .map(|b| b.handle)
            .collect()
    }
}

impl Default for MetaBlobManager {
    fn default() -> Self {
        Self::new()
    }
}

/// FNV-1a hash (64-bit). Simple, fast hash for deduplication.
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manager() {
        let mgr = MetaBlobManager::new();
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_acquire_blob() {
        let mut mgr = MetaBlobManager::new();
        let data = [0u8, 1, 2, 3];
        let handle = mgr.acquire(&data);

        assert!(handle > 0);
        assert_eq!(mgr.count(), 1);

        let blob = mgr.get(handle).unwrap();
        assert_eq!(blob.data, data);
        assert_eq!(blob.size(), 4);
    }

    #[test]
    fn test_deduplication() {
        let mut mgr = MetaBlobManager::new();
        let data = [1u8, 2, 3, 4];

        let h1 = mgr.acquire(&data);
        let h2 = mgr.acquire(&data);

        assert_eq!(h1, h2); // Same handle for identical data.
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_different_data_different_blobs() {
        let mut mgr = MetaBlobManager::new();
        let h1 = mgr.acquire(&[1u8, 2, 3]);
        let h2 = mgr.acquire(&[4u8, 5, 6]);

        assert_ne!(h1, h2);
        assert_eq!(mgr.count(), 2);
    }

    #[test]
    fn test_release_blob() {
        let mut mgr = MetaBlobManager::new();
        let handle = mgr.acquire(&[1u8, 2, 3]);

        assert!(mgr.release(handle));
        assert_eq!(mgr.count(), 0);
        assert!(mgr.get(handle).is_none());
    }

    #[test]
    fn test_double_release_fails() {
        let mut mgr = MetaBlobManager::new();
        let handle = mgr.acquire(&[1u8, 2, 3]);

        assert!(mgr.release(handle));
        assert!(!mgr.release(handle)); // Already released.
    }

    #[test]
    fn test_release_unknown_fails() {
        let mut mgr = MetaBlobManager::new();
        assert!(!mgr.release(999));
    }

    #[test]
    fn test_destroy_all() {
        let mut mgr = MetaBlobManager::new();
        mgr.acquire(&[1u8]);
        mgr.acquire(&[2u8]);
        mgr.acquire(&[3u8]);
        assert_eq!(mgr.count(), 3);

        mgr.destroy_all();
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_handles() {
        let mut mgr = MetaBlobManager::new();
        let h1 = mgr.acquire(&[1u8]);
        let h2 = mgr.acquire(&[2u8]);

        let handles = mgr.handles();
        assert_eq!(handles.len(), 2);
        assert!(handles.contains(&h1));
        assert!(handles.contains(&h2));
    }

    #[test]
    fn test_acquire_after_release() {
        let mut mgr = MetaBlobManager::new();
        let data = [1u8, 2, 3];
        let h1 = mgr.acquire(&data);
        mgr.release(h1);

        // Re-acquire same data: should get a new handle.
        let h2 = mgr.acquire(&data);
        assert_ne!(h1, h2);
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_large_blob() {
        let mut mgr = MetaBlobManager::new();
        let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let handle = mgr.acquire(&data);

        let blob = mgr.get(handle).unwrap();
        assert_eq!(blob.size(), 4096);
    }

    #[test]
    fn test_empty_data_blob() {
        let mut mgr = MetaBlobManager::new();
        let handle = mgr.acquire(&[]);
        assert!(handle > 0);
        assert_eq!(mgr.get(handle).unwrap().size(), 0);
    }

    #[test]
    fn test_drm_mode() {
        let mgr = MetaBlobManager::new_with_drm();
        assert_eq!(mgr.count(), 0);
        // Just verify it doesn't crash in DRM mode.
    }
}
