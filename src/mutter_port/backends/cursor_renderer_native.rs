//! Cursor Renderer Native ported from GNOME Mutter's src/backends/
//!
//! Native backend cursor renderer using hardware cursors via DRM/KMS.
//! Manages hardware cursor planes and animation timers for native display outputs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-renderer-native.c

use alloc::boxed::Box;
use core::ffi::c_void;

/// Opaque backend reference.
pub struct MetaBackend;

/// Opaque Clutter cursor reference.
pub struct ClutterCursor;

/// Opaque cursor sprite reference (upstream `MetaCursorSprite`).
pub struct MetaCursorSprite;

/// Handle to a DRM hardware cursor buffer (GBM BO or dumb buffer handle).
/// In a full implementation this would be a `MetaDrmBuffer` pointer; here it
/// is tracked as a raw handle value so the renderer can record which buffer
/// the current sprite was uploaded into.
pub type BufferHandle = u64;

/// Native DRM/KMS hardware cursor renderer.
///
/// Manages hardware cursor planes via DRM/KMS. The renderer caches the
/// currently-displayed cursor sprite and the hardware buffer it was uploaded
/// to, so it can decide whether a re-upload is needed on each frame.
/// A full implementation would call `drmModeSetCursor` / atomic cursor plane
/// commits to push the buffer to the kernel; those ioctls are documented in
/// `prepare_frame` but not issued here (no DRM file descriptor in `no_std`).
pub struct MetaCursorRendererNative {
    /// Reference to the backend (opaque).
    pub backend: *mut MetaBackend,
    /// Current Clutter cursor object.
    pub current_cursor: *mut ClutterCursor,
    /// Signal handler ID for texture changes.
    pub texture_changed_handler_id: u64,
    /// Animation timeout ID for cursor updates.
    pub animation_timeout_id: u32,
    /// Signal handler ID for pointer position changes.
    pub pointer_position_changed_handler_id: u64,
    /// Flag indicating input thread is disconnected.
    pub input_disconnected: bool,
    /// The cursor sprite currently uploaded to the hardware buffer, or
    /// `None` if no sprite has been uploaded yet. When a new sprite
    /// arrives (`set_current_sprite`) this is compared to decide whether
    /// the buffer needs to be re-rendered.
    current_sprite: *mut MetaCursorSprite,
    /// Handle to the hardware buffer holding the rendered cursor pixels.
    /// `0` means no buffer has been allocated/uploaded yet.
    buffer_handle: BufferHandle,
    /// Set to `true` whenever the sprite changes or the buffer is
    /// invalidated; cleared after `prepare_frame` performs the upload.
    /// The renderer checks this to avoid redundant DRM cursor plane
    /// commits when the sprite hasn't changed between frames.
    needs_redraw: bool,
}

impl MetaCursorRendererNative {
    /// Create a new native cursor renderer.
    pub fn new() -> Self {
        MetaCursorRendererNative {
            backend: core::ptr::null_mut(),
            current_cursor: core::ptr::null_mut(),
            texture_changed_handler_id: 0,
            animation_timeout_id: 0,
            pointer_position_changed_handler_id: 0,
            input_disconnected: false,
            current_sprite: core::ptr::null_mut(),
            buffer_handle: 0,
            needs_redraw: true,
        }
    }

    /// Returns the cursor sprite currently uploaded to the hardware buffer.
    pub fn get_current_sprite(&self) -> *mut MetaCursorSprite {
        self.current_sprite
    }

    /// Sets the cursor sprite to render. If the sprite differs from the one
    /// already uploaded, `needs_redraw` is set so `prepare_frame` will
    /// re-render the sprite to the hardware buffer.
    pub fn set_current_sprite(&mut self, sprite: *mut MetaCursorSprite) {
        if sprite != self.current_sprite {
            self.current_sprite = sprite;
            self.needs_redraw = true;
        }
    }

    /// Returns the handle of the hardware buffer holding the cursor pixels,
    /// or `0` if no buffer has been allocated yet.
    pub fn get_buffer_handle(&self) -> BufferHandle {
        self.buffer_handle
    }

    /// Sets the hardware buffer handle. Called after allocating/uploading a
    /// new cursor buffer (e.g. via GBM `gbm_bo_create` or a DRM dumb buffer
    /// ioctl in a full implementation).
    pub fn set_buffer_handle(&mut self, handle: BufferHandle) {
        self.buffer_handle = handle;
    }

    /// Returns whether the cursor needs to be re-rendered to the hardware
    /// buffer this frame.
    pub fn get_needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Marks the cursor as needing a redraw (e.g. after a texture-change
    /// signal or when the buffer is invalidated by a modeset).
    pub fn set_needs_redraw(&mut self, needs_redraw: bool) {
        self.needs_redraw = needs_redraw;
    }

    /// Prepare cursor frame for renderer view.
    ///
    /// If `needs_redraw` is set and a sprite is present, the sprite would be
    /// rendered to the hardware cursor buffer here. A full implementation
    /// would:
    /// 1. Read the sprite's pixel data (`meta_cursor_sprite_get_cogl_texture`).
    /// 2. Upload it to a GBM BO or DRM dumb buffer (`gbm_bo_map`/`drmModeMapDumb`).
    /// 3. Call `drmModeSetCursor` or build an atomic cursor-plane commit
    ///    referencing `buffer_handle`.
    /// After the upload the redraw flag is cleared so subsequent frames with
    /// the same sprite skip the re-upload.
    pub fn prepare_frame(&mut self) {
        if self.needs_redraw && !self.current_sprite.is_null() {
            // Render cursor sprite to hardware buffer.
            // The buffer handle would be updated by the upload path above;
            // here we record that the upload was logically performed and
            // clear the pending-redraw flag.
            self.needs_redraw = false;
        }
    }
}

impl Default for MetaCursorRendererNative {
    fn default() -> Self {
        Self::new()
    }
}
