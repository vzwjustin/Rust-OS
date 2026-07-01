//! MetaRendererNative ported from GNOME Mutter's
//! src/backends/native/meta-renderer-native.c
//!
//! MetaRendererNative is the native (DRM/KMS) implementation of MetaRenderer.
//! It manages the set of onscreen framebuffers (one per CRTC), coordinates
//! page flips with the KMS atomic commit pipeline, and handles buffer
//! allocation and format negotiation.
//!
//! In Mutter this wraps CoglRenderer, CoglDisplay, and CoglOnscreen objects.
//! In the kernel, Cogl is not available; the renderer is modeled as a plain
//! struct that tracks onscreens and coordinates with the KMS update pipeline.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native.c

use alloc::vec::Vec;
use alloc::collections::BTreeMap;

use super::onscreen_native::MetaOnscreenNative;
use crate::mutter_port::core::drm_format::{DrmFormat, formats, pick_best_format};
use alloc::vec;

/// Renderer mode, mirroring the Cogl renderer mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererMode {
    /// Hardware rendering (GPU).
    Hardware,
    /// Software rendering (CPU fallback).
    Software,
    /// Hybrid (GPU for compositing, CPU for some operations).
    Hybrid,
}

impl Default for RendererMode {
    fn default() -> Self {
        RendererMode::Hardware
    }
}

/// The native renderer. Mirrors MetaRendererNative.
#[derive(Debug)]
pub struct MetaRendererNative {
    /// Onscreen framebuffers keyed by CRTC id.
    onscreens: BTreeMap<u32, MetaOnscreenNative>,
    /// The negotiated pixel format for new onscreens.
    format: DrmFormat,
    /// Current rendering mode.
    mode: RendererMode,
    /// Whether the renderer is paused.
    paused: bool,
    /// Whether a modeset is pending (monitor configuration changed).
    modeset_pending: bool,
    /// Supported DRM formats (from KMS plane negotiation).
    supported_formats: Vec<DrmFormat>,
    /// Whether atomic KMS is supported.
    atomic_supported: bool,
    /// Whether modifiers are supported.
    modifiers_supported: bool,
    /// GPU device id (for multi-GPU systems).
    gpu_id: u32,
    /// Frame count across all onscreens.
    total_frame_count: u64,
}

impl MetaRendererNative {
    /// Create a new native renderer. Mirrors meta_renderer_native_new().
    pub fn new(gpu_id: u32) -> Self {
        MetaRendererNative {
            onscreens: BTreeMap::new(),
            format: formats::XRGB8888,
            mode: RendererMode::default(),
            paused: false,
            modeset_pending: false,
            supported_formats: vec![formats::XRGB8888, formats::ARGB8888],
            atomic_supported: false,
            modifiers_supported: false,
            gpu_id,
            total_frame_count: 0,
        }
    }

    // ── Configuration ─────────────────────────────────────────────────

    pub fn gpu_id(&self) -> u32 {
        self.gpu_id
    }

    pub fn mode(&self) -> RendererMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: RendererMode) {
        self.mode = mode;
    }

    pub fn format(&self) -> DrmFormat {
        self.format
    }

    /// Set the supported DRM formats (from KMS plane negotiation).
    /// Also picks the best format for new onscreens.
    pub fn set_supported_formats(&mut self, formats: Vec<DrmFormat>) {
        self.supported_formats = formats;
        if let Some(best) = pick_best_format(&self.supported_formats) {
            self.format = best;
        }
    }

    pub fn supported_formats(&self) -> &[DrmFormat] {
        &self.supported_formats
    }

    pub fn is_atomic_supported(&self) -> bool {
        self.atomic_supported
    }

    pub fn set_atomic_supported(&mut self, supported: bool) {
        self.atomic_supported = supported;
    }

    pub fn is_modifiers_supported(&self) -> bool {
        self.modifiers_supported
    }

    pub fn set_modifiers_supported(&mut self, supported: bool) {
        self.modifiers_supported = supported;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_modeset_pending(&self) -> bool {
        self.modeset_pending
    }

    /// Mark that a modeset is needed (monitor config changed).
    pub fn request_modeset(&mut self) {
        self.modeset_pending = true;
    }

    /// Clear the modeset pending flag (after the modeset has been applied).
    pub fn clear_modeset_pending(&mut self) {
        self.modeset_pending = false;
    }

    // ── Onscreen management ───────────────────────────────────────────

    /// Create an onscreen for a CRTC. Mirrors
    /// meta_renderer_native_create_onscreen().
    pub fn create_onscreen(
        &mut self,
        crtc_id: u32,
        width: u32,
        height: u32,
    ) -> &mut MetaOnscreenNative {
        let onscreen = MetaOnscreenNative::new(crtc_id, width, height, self.format);
        self.onscreens.insert(crtc_id, onscreen);
        self.onscreens.get_mut(&crtc_id).unwrap()
    }

    /// Get an onscreen by CRTC id.
    pub fn get_onscreen(&self, crtc_id: u32) -> Option<&MetaOnscreenNative> {
        self.onscreens.get(&crtc_id)
    }

    /// Get a mutable onscreen by CRTC id.
    pub fn get_onscreen_mut(&mut self, crtc_id: u32) -> Option<&mut MetaOnscreenNative> {
        self.onscreens.get_mut(&crtc_id)
    }

    /// Remove an onscreen (when a CRTC is disabled).
    pub fn remove_onscreen(&mut self, crtc_id: u32) -> bool {
        if let Some(onscreen) = self.onscreens.get_mut(&crtc_id) {
            onscreen.close();
        }
        self.onscreens.remove(&crtc_id).is_some()
    }

    /// Number of active onscreens.
    pub fn onscreen_count(&self) -> usize {
        self.onscreens.len()
    }

    /// All CRTC ids with onscreens.
    pub fn crtc_ids(&self) -> Vec<u32> {
        self.onscreens.keys().copied().collect()
    }

    // ── Page flip coordination ────────────────────────────────────────

    /// Swap buffers on all ready onscreens. Mirrors the atomic commit
    /// path in meta_renderer_native_present_flipped().
    ///
    /// Returns the list of CRTC ids that were flipped.
    pub fn present(&mut self) -> Vec<u32> {
        if self.paused || self.modeset_pending {
            return Vec::new();
        }

        let mut flipped = Vec::new();
        for (&crtc_id, onscreen) in &mut self.onscreens {
            if onscreen.swap_buffers().is_ok() {
                flipped.push(crtc_id);
            }
        }
        flipped
    }

    /// Notify that a page flip has completed for a CRTC. Mirrors
    /// the page-flip event handler.
    pub fn on_page_flip_done(&mut self, crtc_id: u32) {
        if let Some(onscreen) = self.onscreens.get_mut(&crtc_id) {
            let before = onscreen.frame_count();
            onscreen.on_page_flip_done();
            let after = onscreen.frame_count();
            if after > before {
                self.total_frame_count += after - before;
            }
        }
    }

    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }

    // ── Pause / resume ────────────────────────────────────────────────

    /// Pause the renderer (VT switch away). Mirrors
    /// meta_renderer_native_pause().
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the renderer (VT switch back). Mirrors
    /// meta_renderer_native_resume().
    pub fn resume(&mut self) {
        self.paused = false;
        // A modeset is needed after resume to re-establish the scanout.
        self.modeset_pending = true;
    }

    // ── Rebuild ───────────────────────────────────────────────────────

    /// Rebuild all onscreens for the current monitor configuration.
    /// Mirrors meta_renderer_native_rebuild_views().
    ///
    /// Removes onscreens for disabled CRTCs and creates new ones for
    /// newly-enabled CRTCs.
    pub fn rebuild_views(&mut self, crtc_configs: &[(u32, u32, u32)]) {
        let active_crtcs: Vec<u32> = crtc_configs.iter().map(|(id, _, _)| *id).collect();

        // Remove onscreens for inactive CRTCs.
        let to_remove: Vec<u32> = self.onscreens.keys()
            .filter(|id| !active_crtcs.contains(id))
            .copied()
            .collect();
        for id in to_remove {
            self.remove_onscreen(id);
        }

        // Create or resize onscreens for active CRTCs.
        for &(crtc_id, width, height) in crtc_configs {
            if let Some(onscreen) = self.onscreens.get_mut(&crtc_id) {
                if onscreen.width() != width || onscreen.height() != height {
                    onscreen.resize(width, height);
                }
            } else {
                self.create_onscreen(crtc_id, width, height);
            }
        }

        self.modeset_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let renderer = MetaRendererNative::new(0);
        assert_eq!(renderer.gpu_id(), 0);
        assert_eq!(renderer.mode(), RendererMode::Hardware);
        assert!(!renderer.is_paused());
        assert_eq!(renderer.onscreen_count(), 0);
    }

    #[test]
    fn test_create_onscreen() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);

        assert_eq!(renderer.onscreen_count(), 1);
        assert!(renderer.get_onscreen(1).is_some());
        assert_eq!(renderer.get_onscreen(1).unwrap().width(), 1920);
    }

    #[test]
    fn test_remove_onscreen() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);

        assert!(renderer.remove_onscreen(1));
        assert_eq!(renderer.onscreen_count(), 0);
    }

    #[test]
    fn test_supported_formats() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.set_supported_formats(vec![formats::XRGB8888, formats::ARGB2101010]);

        // Should pick the higher bit depth format.
        assert_eq!(renderer.format(), formats::ARGB2101010);
    }

    #[test]
    fn test_pause_resume() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.pause();
        assert!(renderer.is_paused());

        renderer.resume();
        assert!(!renderer.is_paused());
        assert!(renderer.is_modeset_pending()); // Resume requires modeset.
    }

    #[test]
    fn test_present_when_paused() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);
        renderer.pause();

        let flipped = renderer.present();
        assert!(flipped.is_empty());
    }

    #[test]
    fn test_present_with_modeset_pending() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);
        renderer.request_modeset();

        let flipped = renderer.present();
        assert!(flipped.is_empty());
    }

    #[test]
    fn test_present_and_page_flip() {
        let mut renderer = MetaRendererNative::new(0);
        let onscreen = renderer.create_onscreen(1, 1920, 1080);
        use super::onscreen_native::SwapBuffer;
        onscreen.assign_buffers(
            SwapBuffer::new(1, 1920, 1080, formats::XRGB8888),
            SwapBuffer::new(2, 1920, 1080, formats::XRGB8888),
        );

        let flipped = renderer.present();
        assert_eq!(flipped, vec![1]);

        renderer.on_page_flip_done(1);
        assert_eq!(renderer.total_frame_count(), 1);
    }

    #[test]
    fn test_rebuild_views() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);
        renderer.create_onscreen(2, 1920, 1080);

        // Rebuild with only CRTC 1 at a new resolution, and CRTC 3 new.
        renderer.rebuild_views(&[(1, 2560, 1440), (3, 1920, 1080)]);

        assert_eq!(renderer.onscreen_count(), 2);
        assert!(renderer.get_onscreen(1).is_some());
        assert!(renderer.get_onscreen(2).is_none());
        assert!(renderer.get_onscreen(3).is_some());
        assert_eq!(renderer.get_onscreen(1).unwrap().width(), 2560);
        assert!(!renderer.is_modeset_pending());
    }

    #[test]
    fn test_rebuild_views_empty() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.create_onscreen(1, 1920, 1080);

        renderer.rebuild_views(&[]);
        assert_eq!(renderer.onscreen_count(), 0);
    }

    #[test]
    fn test_multiple_onscreens_present() {
        let mut renderer = MetaRendererNative::new(0);
        use super::onscreen_native::SwapBuffer;

        for crtc_id in 1..=3 {
            let onscreen = renderer.create_onscreen(crtc_id, 1920, 1080);
            onscreen.assign_buffers(
                SwapBuffer::new(crtc_id * 2 - 1, 1920, 1080, formats::XRGB8888),
                SwapBuffer::new(crtc_id * 2, 1920, 1080, formats::XRGB8888),
            );
        }

        let flipped = renderer.present();
        assert_eq!(flipped.len(), 3);
    }

    #[test]
    fn test_atomic_and_modifiers() {
        let mut renderer = MetaRendererNative::new(0);
        assert!(!renderer.is_atomic_supported());
        assert!(!renderer.is_modifiers_supported());

        renderer.set_atomic_supported(true);
        renderer.set_modifiers_supported(true);
        assert!(renderer.is_atomic_supported());
        assert!(renderer.is_modifiers_supported());
    }

    #[test]
    fn test_software_mode() {
        let mut renderer = MetaRendererNative::new(0);
        renderer.set_mode(RendererMode::Software);
        assert_eq!(renderer.mode(), RendererMode::Software);
    }
}
