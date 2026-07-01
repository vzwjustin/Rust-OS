//! Renderer ported from GNOME Mutter's src/backends/meta-renderer.c
//!
//! Keeps track of the different renderer views. A renderer has two jobs:
//!   1. Maintain a list of `RendererView`s, one per logical monitor, each
//!      responsible for rendering the part of the stage on that monitor.
//!   2. Create and set up an appropriate Cogl renderer (backend-specific).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-renderer.c

use alloc::vec::Vec;

use super::renderer_view::RendererView;

/// A renderer: owns the set of renderer views covering the global stage.
#[derive(Debug)]
pub struct Renderer {
    /// Opaque backend handle (MetaBackend pointer in Mutter). Stubbed as id.
    pub backend_id: u64,
    views: Vec<RendererView>,
    is_paused: bool,
}

impl Renderer {
    pub fn new(backend_id: u64) -> Self {
        Renderer {
            backend_id,
            views: Vec::new(),
            is_paused: false,
        }
    }

    pub fn get_backend_id(&self) -> u64 {
        self.backend_id
    }

    /// Create a Cogl renderer appropriate for this backend.
    ///
    /// Stub: in Mutter this is a virtual method returning a CoglRenderer, which
    /// may install a custom winsys for swapBuffers/vsync. No Cogl in-kernel.
    pub fn create_cogl_renderer(&self) {
        // Backend-specific CoglRenderer creation; stubbed for no_std kernel.
    }

    /// Add a view to the renderer.
    ///
    /// Faithful port of meta_renderer_add_view: while paused, the new view's
    /// frame clock is inhibited so it does not schedule updates.
    pub fn add_view(&mut self, view: RendererView) {
        let inhibit = self.is_paused;
        self.views.push(view);
        if inhibit {
            // clutter_frame_clock_inhibit(view frame clock) - stubbed.
        }
    }

    /// Rebuild the internal list of views by querying the monitor manager.
    ///
    /// Stub for the parts that touch Clutter/Cogl and the live monitor manager;
    /// the caller supplies the freshly-built views. This mirrors
    /// meta_renderer_real_rebuild_views, which frees old views then creates one
    /// view per CRTC of each logical monitor.
    pub fn rebuild_views(&mut self, new_views: Vec<RendererView>) {
        // Old views would be destroyed via clutter_stage_view_destroy.
        self.views = new_views;
    }

    /// Return the view responsible for the given CRTC, if any.
    /// Faithful port of meta_renderer_get_view_for_crtc.
    pub fn get_view_for_crtc(&self, crtc_id: u64) -> Option<&RendererView> {
        self.views.iter().find(|v| v.get_crtc_id() == Some(crtc_id))
    }

    /// Return all views, each dealing with a part of the stage.
    pub fn get_views(&self) -> &[RendererView] {
        &self.views
    }

    /// Pause the renderer, inhibiting every view's frame clock.
    /// Faithful port of meta_renderer_pause (must not be already paused).
    pub fn pause(&mut self) {
        debug_assert!(!self.is_paused);
        self.is_paused = true;
        for _view in &self.views {
            // clutter_frame_clock_inhibit(stage_view frame clock) - stubbed.
        }
    }

    /// Resume the renderer, uninhibiting every view's frame clock.
    /// Faithful port of meta_renderer_resume (must be currently paused).
    pub fn resume(&mut self) {
        debug_assert!(self.is_paused);
        self.is_paused = false;
        for _view in &self.views {
            // clutter_frame_clock_uninhibit(stage_view frame clock) - stubbed.
        }
        // Backend-specific klass->resume(renderer) would run here.
    }

    /// Whether the renderer is hardware accelerated.
    ///
    /// Stub: queries the Cogl driver in Mutter. Reports false (software) in the
    /// absence of a real GPU driver binding.
    pub fn is_hardware_accelerated(&self) -> bool {
        false
    }
}
