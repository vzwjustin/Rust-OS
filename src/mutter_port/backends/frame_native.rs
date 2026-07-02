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

    /// Set the DRM buffer for this frame.
    pub fn set_buffer(&mut self, buffer: *mut MetaDrmBuffer) {
        self.buffer = buffer;
    }

    /// Get the DRM buffer, if any.
    pub fn get_buffer(&self) -> Option<*mut MetaDrmBuffer> {
        if self.buffer.is_null() {
            None
        } else {
            Some(self.buffer)
        }
    }

    /// Set the Cogl scanout for this frame.
    pub fn set_scanout(&mut self, scanout: *mut CoglScanout) {
        self.scanout = scanout;
    }

    /// Get the Cogl scanout, if any.
    pub fn get_scanout(&self) -> Option<*mut CoglScanout> {
        if self.scanout.is_null() {
            None
        } else {
            Some(self.scanout)
        }
    }

    /// Set the KMS update for this frame.
    pub fn set_kms_update(&mut self, update: *mut MetaKmsUpdate) {
        self.kms_update = update;
    }

    /// Steal (take ownership of) the KMS update. Returns the raw
    /// pointer and clears the frame's reference.
    pub fn steal_kms_update(&mut self) -> Option<*mut MetaKmsUpdate> {
        if self.kms_update.is_null() {
            None
        } else {
            let update = self.kms_update;
            self.kms_update = core::ptr::null_mut();
            Some(update)
        }
    }

    /// Whether this frame has a pending KMS update.
    pub fn has_kms_update(&self) -> bool {
        !self.kms_update.is_null()
    }

    /// Set the damage region.
    pub fn set_damage(&mut self, damage: *mut MtkRegion) {
        self.damage = damage;
    }

    /// Get the damage region, if any.
    pub fn get_damage(&self) -> Option<*mut MtkRegion> {
        if self.damage.is_null() {
            None
        } else {
            Some(self.damage)
        }
    }

    /// Set the sync file descriptor.
    pub fn set_sync_fd(&mut self, sync_fd: i32) {
        self.sync_fd = sync_fd;
    }

    /// Steal (take ownership of) the sync file descriptor. Returns
    /// the fd and resets the frame's fd to -1.
    pub fn steal_sync_fd(&mut self) -> Option<i32> {
        if self.sync_fd < 0 {
            None
        } else {
            let fd = self.sync_fd;
            self.sync_fd = -1;
            Some(fd)
        }
    }

    /// Whether the frame is ready for presentation (has a buffer or
    /// scanout, and no pending sync events).
    pub fn is_ready(&self) -> bool {
        (!self.buffer.is_null() || !self.scanout.is_null()) && self.sync_events == 0
    }
}

impl Default for MetaFrameNative {
    fn default() -> Self {
        Self::new()
    }
}
