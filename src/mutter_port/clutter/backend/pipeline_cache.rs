//! Port of GNOME mutter's `clutter/clutter-pipeline-cache.{c,h}`.
//!
//! LRU cache that deduplicates CoglPipeline objects by configuration state.
//!
//! # What's ported
//!
//! - `PipelineCache` struct with LRU eviction and configurable capacity.
//! - `PipelineKey` struct: blend mode, color, texture, snippet hash.
//! - `get_or_create`: lookup or insert with LRU update.
//! - `get`: lookup without creating.
//! - `clear`, `len`, `is_empty`, `capacity`, `contains`, `keys`.
//! - `evict_least_recently_used`.
//!
//! # What's skipped
//!
//! - `CoglPipeline` creation: opaque `u32` handles allocated sequentially.
//! - `GHashTable` internals: replaced by `Vec`-based LRU.
//! - Snippet caching: modeled as opaque `u32` hash.

#![allow(dead_code)]

use alloc::vec::Vec;

use super::paint_nodes::{PipelineHandle, TextureHandle};
use super::super::paint_node::Rgba;

pub const DEFAULT_CAPACITY: usize = 64;

/// Blend mode mirroring Cogl blend state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    None = 0,
    AlphaBlend = 1,
    Premultiplied = 2,
    Additive = 3,
    SourceOver = 4,
}

/// Key identifying a cached pipeline by configuration state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub blend_mode: BlendMode,
    pub color: Rgba,
    pub texture: Option<TextureHandle>,
    pub snippet_hash: u32,
}

impl PipelineKey {
    pub const fn new(blend_mode: BlendMode, color: Rgba, texture: Option<TextureHandle>, snippet_hash: u32) -> Self {
        PipelineKey { blend_mode, color, texture, snippet_hash }
    }
    pub const fn solid(color: Rgba) -> Self {
        PipelineKey::new(BlendMode::None, color, None, 0)
    }
    pub const fn textured(texture: TextureHandle) -> Self {
        PipelineKey::new(BlendMode::Premultiplied, Rgba::new(255, 255, 255, 255), Some(texture), 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheEntry {
    key: PipelineKey,
    pipeline: PipelineHandle,
}

/// Port of `ClutterPipelineCache` — LRU cache of CoglPipeline objects.
pub struct PipelineCache {
    entries: Vec<CacheEntry>,
    capacity: usize,
    next_handle: u32,
}

impl Default for PipelineCache {
    fn default() -> Self { Self::new(DEFAULT_CAPACITY) }
}

impl PipelineCache {
    pub fn new(capacity: usize) -> Self {
        PipelineCache {
            entries: Vec::new(),
            capacity: if capacity == 0 { 1 } else { capacity },
            next_handle: 1,
        }
    }

    pub fn capacity(&self) -> usize { self.capacity }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn allocate_handle(&mut self) -> PipelineHandle {
        let handle = PipelineHandle(self.next_handle);
        self.next_handle = self.next_handle.saturating_add(1);
        handle
    }

    pub fn get_or_create(&mut self, key: PipelineKey) -> PipelineHandle {
        if let Some(pos) = self.entries.iter().position(|e| e.key == key) {
            let entry = self.entries.remove(pos);
            self.entries.insert(0, entry);
            return entry.pipeline;
        }
        let pipeline = self.allocate_handle();
        let entry = CacheEntry { key, pipeline };
        if self.entries.len() >= self.capacity { self.entries.pop(); }
        self.entries.insert(0, entry);
        pipeline
    }

    pub fn get(&mut self, key: &PipelineKey) -> Option<PipelineHandle> {
        if let Some(pos) = self.entries.iter().position(|e| e.key == *key) {
            let entry = self.entries.remove(pos);
            self.entries.insert(0, entry);
            Some(entry.pipeline)
        } else {
            None
        }
    }

    pub fn evict_least_recently_used(&mut self) -> Option<(PipelineKey, PipelineHandle)> {
        self.entries.pop().map(|e| (e.key, e.pipeline))
    }

    pub fn clear(&mut self) { self.entries.clear(); }

    pub fn contains(&self, key: &PipelineKey) -> bool {
        self.entries.iter().any(|e| e.key == *key)
    }

    pub fn keys(&self) -> Vec<PipelineKey> {
        self.entries.iter().map(|e| e.key).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Rgba { Rgba::new(255, 0, 0, 255) }
    fn blue() -> Rgba { Rgba::new(0, 0, 255, 255) }

    #[test]
    fn new_cache_is_empty() {
        let cache = PipelineCache::new(8);
        assert!(cache.is_empty());
        assert_eq!(cache.capacity(), 8);
    }

    #[test]
    fn default_capacity() {
        let cache = PipelineCache::default();
        assert_eq!(cache.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn get_or_create_inserts_on_miss() {
        let mut cache = PipelineCache::new(8);
        let key = PipelineKey::solid(red());
        let handle = cache.get_or_create(key);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&key));
        assert_eq!(handle, PipelineHandle(1));
    }

    #[test]
    fn get_or_create_returns_same_on_hit() {
        let mut cache = PipelineCache::new(8);
        let key = PipelineKey::solid(red());
        let h1 = cache.get_or_create(key);
        let h2 = cache.get_or_create(key);
        assert_eq!(h1, h2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn different_keys_get_different_handles() {
        let mut cache = PipelineCache::new(8);
        let h1 = cache.get_or_create(PipelineKey::solid(red()));
        let h2 = cache.get_or_create(PipelineKey::solid(blue()));
        assert_ne!(h1, h2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn lru_eviction_when_at_capacity() {
        let mut cache = PipelineCache::new(3);
        let k1 = PipelineKey::solid(red());
        let k2 = PipelineKey::solid(blue());
        let k3 = PipelineKey::new(BlendMode::AlphaBlend, red(), None, 0);
        let k4 = PipelineKey::new(BlendMode::Additive, blue(), None, 0);
        cache.get_or_create(k1);
        cache.get_or_create(k2);
        cache.get_or_create(k3);
        assert_eq!(cache.len(), 3);
        cache.get_or_create(k4);
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains(&k1));
        assert!(cache.contains(&k4));
    }

    #[test]
    fn cache_hit_moves_to_mru_preventing_eviction() {
        let mut cache = PipelineCache::new(3);
        let k1 = PipelineKey::solid(red());
        let k2 = PipelineKey::solid(blue());
        let k3 = PipelineKey::new(BlendMode::AlphaBlend, red(), None, 0);
        let k4 = PipelineKey::new(BlendMode::Additive, blue(), None, 0);
        cache.get_or_create(k1);
        cache.get_or_create(k2);
        cache.get_or_create(k3);
        cache.get_or_create(k1); // move k1 to MRU
        cache.get_or_create(k4);
        assert!(cache.contains(&k1));
        assert!(!cache.contains(&k2));
    }

    #[test]
    fn evict_lru_returns_entry() {
        let mut cache = PipelineCache::new(8);
        let k1 = PipelineKey::solid(red());
        let k2 = PipelineKey::solid(blue());
        cache.get_or_create(k1);
        cache.get_or_create(k2);
        let evicted = cache.evict_least_recently_used();
        assert!(evicted.is_some());
        assert_eq!(evicted.unwrap().0, k1);
    }

    #[test]
    fn clear_empties_cache() {
        let mut cache = PipelineCache::new(8);
        cache.get_or_create(PipelineKey::solid(red()));
        cache.get_or_create(PipelineKey::solid(blue()));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn textured_key_factory() {
        let key = PipelineKey::textured(TextureHandle(7));
        assert_eq!(key.texture, Some(TextureHandle(7)));
        assert_eq!(key.blend_mode, BlendMode::Premultiplied);
    }

    #[test]
    fn many_inserts_respect_capacity() {
        let mut cache = PipelineCache::new(4);
        for i in 0..10u8 {
            cache.get_or_create(PipelineKey::new(BlendMode::None, Rgba::new(i, 0, 0, 255), None, 0));
        }
        assert_eq!(cache.len(), 4);
    }
}
