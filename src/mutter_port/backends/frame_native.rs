//! Frame Native ported from GNOME Mutter's src/backends/
//!
//! Represents a native frame in the display pipeline with KMS, DRM buffer,
//! scanout, and damage region tracking for hardware-accelerated rendering.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-frame-native.h

/// Opaque KMS update type from upstream.
pub struct MetaKmsUpdate;

/// Opaque KMS device type from upstream.
pub struct MetaKmsDevice;

/// Opaque DRM buffer type from upstream.
pub struct MetaDrmBuffer;

/// Opaque Cogl scanout type from upstream.
pub struct CoglScanout;

/// Opaque MTK region type from upstream.
pub struct MtkRegion;

/// Opaque Clutter frame type from upstream.
pub struct ClutterFrame;

/// Opaque GSource type from upstream.
pub struct GSource;

/// Native frame with KMS update and buffer state.
/// Carries rendering metadata for the frame pipeline.
/// Stores DRM buffer references, KMS update state, damage regions, and sync file descriptors.
pub struct MetaFrameNative {
    pub buffer: *mut MetaDrmBuffer,
    pub scanout: *mut CoglScanout,
    pub kms_update: *mut MetaKmsUpdate,
    pub damage: *mut MtkRegion,
    pub sync_fd: i32,
    pub sync_events: u32,
}

impl MetaFrameNative {
    /// Create a new native frame.
    pub fn new() -> Self {
        MetaFrameNative {
            buffer: core::ptr::null_mut(),
            scanout: core::ptr::null_mut(),
            kms_update: core::ptr::null_mut(),
            damage: core::ptr::null_mut(),
            sync_fd: -1,
            sync_events: 0,
        }
    }
}

impl Default for MetaFrameNative {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: The following would require upstream integration:
// pub fn meta_frame_native_from_frame(frame: &ClutterFrame) -> &MetaFrameNative { ... }
// pub fn meta_frame_native_ensure_kms_update(frame: &mut MetaFrameNative, device: &MetaKmsDevice) -> &mut MetaKmsUpdate { ... }
// pub fn meta_frame_native_steal_kms_update(frame: &mut MetaFrameNative) -> Option<MetaKmsUpdate> { ... }
// pub fn meta_frame_native_has_kms_update(frame: &MetaFrameNative) -> bool { ... }
// pub fn meta_frame_native_set_buffer(frame: &mut MetaFrameNative, buffer: &MetaDrmBuffer) { ... }
// pub fn meta_frame_native_get_buffer(frame: &MetaFrameNative) -> Option<&MetaDrmBuffer> { ... }
// pub fn meta_frame_native_set_scanout(frame: &mut MetaFrameNative, scanout: &CoglScanout) { ... }
// pub fn meta_frame_native_get_scanout(frame: &MetaFrameNative) -> Option<&CoglScanout> { ... }
// pub fn meta_frame_native_set_damage(frame: &mut MetaFrameNative, damage: &MtkRegion) { ... }
// pub fn meta_frame_native_get_damage(frame: &MetaFrameNative) -> Option<&MtkRegion> { ... }
// pub fn meta_frame_native_set_sync_fd(frame: &mut MetaFrameNative, sync_fd: i32) { ... }
// pub fn meta_frame_native_steal_sync_fd(frame: &mut MetaFrameNative) -> Option<i32> { ... }
// pub fn meta_frame_native_add_source(frame: &mut MetaFrameNative, source: &GSource) { ... }
// pub fn meta_frame_native_remove_source(frame: &mut MetaFrameNative, source: &GSource) { ... }
// pub fn meta_frame_native_is_ready(frame: &MetaFrameNative) -> bool { ... }
