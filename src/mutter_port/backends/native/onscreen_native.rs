//! MetaOnscreenNative ported from GNOME Mutter's
//! src/backends/native/meta-onscreen-native.c
//!
//! MetaOnscreenNative is the native (DRM/KMS) implementation of a Cogl
//! onscreen framebuffer. It manages the swap chain for a CRTC: the
//! front buffer currently being scanned out, the back buffer being
//! rendered to, and optional intermediate buffers for atomic commits.
//!
//! In Mutter this wraps CoglFramebuffer, CoglOnscreen, and DRM framebuffer
//! objects. In the kernel, Cogl is not available; the onscreen is modeled
//! as a plain struct that tracks the swap chain state and coordinates
//! with the KMS update pipeline.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-onscreen-native.c

use alloc::vec::Vec;

use crate::mutter_port::core::drm_format::{formats, modifiers, DrmFormat, DrmModifier};

/// Swap buffer state. Mirrors MetaOnscreenNativeState (simplified).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnscreenState {
    /// Initial state, no buffers assigned.
    Initial,
    /// Buffers assigned, ready to render.
    Ready,
    /// Frame submitted, waiting for page flip.
    Flipping,
    /// Page flip completed, frame is now on screen.
    Flipped,
}

/// A framebuffer in the swap chain. Mirrors a DRM framebuffer (drm_fb).
#[derive(Debug, Clone)]
pub struct SwapBuffer {
    /// DRM framebuffer handle (uint32 from DRM_IOCTL_MODE_ADDFB2).
    pub fb_handle: u32,
    /// DMA-BUF file descriptor (for zero-copy).
    pub dma_buf_fd: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format.
    pub format: DrmFormat,
    /// Format modifier (tiling/compression).
    pub modifier: DrmModifier,
    /// Whether this buffer is currently being scanned out by the CRTC.
    pub on_scanout: bool,
    /// Whether this buffer is currently being rendered to.
    pub in_use: bool,
}

impl SwapBuffer {
    pub fn new(fb_handle: u32, width: u32, height: u32, format: DrmFormat) -> Self {
        SwapBuffer {
            fb_handle,
            dma_buf_fd: -1,
            width,
            height,
            format,
            modifier: modifiers::LINEAR,
            on_scanout: false,
            in_use: false,
        }
    }
}

/// The onscreen framebuffer for a native CRTC. Mirrors MetaOnscreenNative.
#[derive(Debug)]
pub struct MetaOnscreenNative {
    /// CRTC id this onscreen is attached to.
    crtc_id: u32,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Pixel format.
    format: DrmFormat,
    /// Current swap chain state.
    state: OnscreenState,
    /// Front buffer (currently on screen).
    front_buffer: Option<SwapBuffer>,
    /// Back buffer (currently being rendered to).
    back_buffer: Option<SwapBuffer>,
    /// Idle/secondary buffers for triple buffering.
    idle_buffers: Vec<SwapBuffer>,
    /// Whether a page flip is pending.
    page_flip_pending: bool,
    /// Frame counter.
    frame_count: u64,
    /// Whether the onscreen has been closed.
    closed: bool,
    /// Closes pending (for deferred cleanup).
    pending_closes: u32,
}

impl MetaOnscreenNative {
    /// Create a new onscreen for a CRTC. Mirrors
    /// meta_onscreen_native_new().
    pub fn new(crtc_id: u32, width: u32, height: u32, format: DrmFormat) -> Self {
        MetaOnscreenNative {
            crtc_id,
            width,
            height,
            format,
            state: OnscreenState::Initial,
            front_buffer: None,
            back_buffer: None,
            idle_buffers: Vec::new(),
            page_flip_pending: false,
            frame_count: 0,
            closed: false,
            pending_closes: 0,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn crtc_id(&self) -> u32 {
        self.crtc_id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> DrmFormat {
        self.format
    }

    pub fn state(&self) -> OnscreenState {
        self.state
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub fn front_buffer(&self) -> Option<&SwapBuffer> {
        self.front_buffer.as_ref()
    }

    pub fn back_buffer(&self) -> Option<&SwapBuffer> {
        self.back_buffer.as_ref()
    }

    pub fn is_page_flip_pending(&self) -> bool {
        self.page_flip_pending
    }

    // ── Swap chain management ─────────────────────────────────────────

    /// Assign the front and back buffers. Mirrors the buffer assignment
    /// in meta_onscreen_native_allocate().
    pub fn assign_buffers(&mut self, front: SwapBuffer, back: SwapBuffer) {
        self.front_buffer = Some(front);
        self.back_buffer = Some(back);
        self.state = OnscreenState::Ready;
    }

    /// Swap front and back buffers (page flip). Mirrors
    /// meta_onscreen_native_swap_buffers().
    ///
    /// After this call, the former back buffer is on screen and the
    /// former front buffer is available for rendering.
    pub fn swap_buffers(&mut self) -> Result<(), &'static str> {
        if self.closed {
            return Err("Onscreen is closed");
        }
        if self.front_buffer.is_none() || self.back_buffer.is_none() {
            return Err("No buffers assigned");
        }
        if self.page_flip_pending {
            return Err("Page flip already pending");
        }

        // Swap: old front goes to idle, old back becomes new front.
        let old_front = self.front_buffer.take().unwrap();
        let mut new_front = self.back_buffer.take().unwrap();

        // Mark new front as on scanout.
        new_front.on_scanout = true;
        new_front.in_use = false;

        // Old front goes to idle (available for next render).
        let mut idle = old_front;
        idle.on_scanout = false;
        idle.in_use = false;
        self.idle_buffers.push(idle);

        self.front_buffer = Some(new_front);
        self.state = OnscreenState::Flipping;
        self.page_flip_pending = true;

        Ok(())
    }

    /// Notify that the page flip has completed. Mirrors the
    /// meta_onscreen_native_page_flip_done() callback.
    pub fn on_page_flip_done(&mut self) {
        if self.page_flip_pending {
            self.page_flip_pending = false;
            self.state = OnscreenState::Flipped;
            self.frame_count += 1;

            // Pick up a new back buffer from the idle pool.
            if let Some(mut buf) = self.idle_buffers.pop() {
                buf.in_use = true;
                self.back_buffer = Some(buf);
                self.state = OnscreenState::Ready;
            }
        }
    }

    /// Get a back buffer for rendering. If no back buffer is assigned,
    /// pull one from the idle pool.
    pub fn acquire_back_buffer(&mut self) -> Option<&SwapBuffer> {
        if self.back_buffer.is_none() {
            if let Some(mut buf) = self.idle_buffers.pop() {
                buf.in_use = true;
                self.back_buffer = Some(buf);
            }
        }
        self.back_buffer.as_ref()
    }

    // ── Resize ────────────────────────────────────────────────────────

    /// Resize the onscreen. Mirrors meta_onscreen_native_resize().
    /// Existing buffers are returned to the idle pool (to be reallocated
    /// at the new size by the backend).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;

        // Return all buffers to idle for reallocation.
        if let Some(front) = self.front_buffer.take() {
            self.idle_buffers.push(front);
        }
        if let Some(back) = self.back_buffer.take() {
            self.idle_buffers.push(back);
        }
        self.state = OnscreenState::Initial;
    }

    // ── Close ─────────────────────────────────────────────────────────

    /// Close the onscreen. Mirrors meta_onscreen_native_destroy().
    /// Buffers are freed; the CRTC is released.
    pub fn close(&mut self) {
        self.closed = true;
        self.front_buffer = None;
        self.back_buffer = None;
        self.idle_buffers.clear();
        self.state = OnscreenState::Initial;
    }

    /// Schedule a deferred close (after pending page flip completes).
    pub fn schedule_close(&mut self) {
        self.pending_closes += 1;
    }

    pub fn pending_close_count(&self) -> u32 {
        self.pending_closes
    }

    /// Process a deferred close if one is pending.
    pub fn process_pending_close(&mut self) -> bool {
        if self.pending_closes > 0 && !self.page_flip_pending {
            self.pending_closes -= 1;
            self.close();
            true
        } else {
            false
        }
    }

    // ── Idle buffer pool ──────────────────────────────────────────────

    /// Number of idle buffers available.
    pub fn idle_buffer_count(&self) -> usize {
        self.idle_buffers.len()
    }

    /// Add a buffer to the idle pool.
    pub fn add_idle_buffer(&mut self, buffer: SwapBuffer) {
        self.idle_buffers.push(buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(handle: u32, w: u32, h: u32) -> SwapBuffer {
        SwapBuffer::new(handle, w, h, formats::XRGB8888)
    }

    #[test]
    fn test_creation() {
        let onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        assert_eq!(onscreen.crtc_id(), 1);
        assert_eq!(onscreen.width(), 1920);
        assert_eq!(onscreen.height(), 1080);
        assert_eq!(onscreen.state(), OnscreenState::Initial);
        assert!(!onscreen.is_closed());
    }

    #[test]
    fn test_assign_buffers() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));
        assert_eq!(onscreen.state(), OnscreenState::Ready);
        assert!(onscreen.front_buffer().is_some());
        assert!(onscreen.back_buffer().is_some());
    }

    #[test]
    fn test_swap_buffers() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));

        assert!(onscreen.swap_buffers().is_ok());
        assert_eq!(onscreen.state(), OnscreenState::Flipping);
        assert!(onscreen.is_page_flip_pending());

        // New front should be the old back (handle 2).
        assert_eq!(onscreen.front_buffer().unwrap().fb_handle, 2);
    }

    #[test]
    fn test_page_flip_done() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));
        onscreen.swap_buffers().unwrap();

        // Add an idle buffer so the back buffer can be re-acquired.
        onscreen.add_idle_buffer(make_buffer(1, 1920, 1080));

        onscreen.on_page_flip_done();
        assert!(!onscreen.is_page_flip_pending());
        assert_eq!(onscreen.frame_count(), 1);
        assert!(onscreen.back_buffer().is_some());
    }

    #[test]
    fn test_swap_without_buffers_fails() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        assert!(onscreen.swap_buffers().is_err());
    }

    #[test]
    fn test_double_swap_fails() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));
        onscreen.swap_buffers().unwrap();
        assert!(onscreen.swap_buffers().is_err()); // Flip pending.
    }

    #[test]
    fn test_resize() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));

        onscreen.resize(2560, 1440);
        assert_eq!(onscreen.width(), 2560);
        assert_eq!(onscreen.height(), 1440);
        assert_eq!(onscreen.state(), OnscreenState::Initial);
        // Buffers should be in idle pool.
        assert_eq!(onscreen.idle_buffer_count(), 2);
    }

    #[test]
    fn test_close() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));

        onscreen.close();
        assert!(onscreen.is_closed());
        assert!(onscreen.front_buffer().is_none());
        assert!(onscreen.back_buffer().is_none());
    }

    #[test]
    fn test_swap_on_closed_fails() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.close();
        assert!(onscreen.swap_buffers().is_err());
    }

    #[test]
    fn test_pending_close() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));
        onscreen.swap_buffers().unwrap();

        onscreen.schedule_close();
        assert_eq!(onscreen.pending_close_count(), 1);

        // Can't close while flip pending.
        assert!(!onscreen.process_pending_close());

        // After flip completes, close can proceed.
        onscreen.add_idle_buffer(make_buffer(1, 1920, 1080));
        onscreen.on_page_flip_done();
        assert!(onscreen.process_pending_close());
        assert!(onscreen.is_closed());
    }

    #[test]
    fn test_acquire_back_buffer_from_idle() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.add_idle_buffer(make_buffer(1, 1920, 1080));

        let buf = onscreen.acquire_back_buffer();
        assert!(buf.is_some());
        assert_eq!(buf.unwrap().fb_handle, 1);
    }

    #[test]
    fn test_multiple_frames() {
        let mut onscreen = MetaOnscreenNative::new(1, 1920, 1080, formats::XRGB8888);
        onscreen.assign_buffers(make_buffer(1, 1920, 1080), make_buffer(2, 1920, 1080));

        // Frame 1.
        onscreen.swap_buffers().unwrap();
        onscreen.add_idle_buffer(make_buffer(1, 1920, 1080));
        onscreen.on_page_flip_done();
        assert_eq!(onscreen.frame_count(), 1);

        // Frame 2.
        onscreen.swap_buffers().unwrap();
        onscreen.add_idle_buffer(make_buffer(2, 1920, 1080));
        onscreen.on_page_flip_done();
        assert_eq!(onscreen.frame_count(), 2);
    }
}
