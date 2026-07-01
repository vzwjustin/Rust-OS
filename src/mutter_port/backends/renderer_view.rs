//! Renderer view ported from GNOME Mutter's src/backends/meta-renderer-view.c
//!
//! Renders (a part of) the global stage — specifically the part that maps to a
//! single logical monitor. It applies the right monitor transform and scaling
//! and manages the view's color state. It derives from `StageView`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-renderer-view.c

use super::stage_view::StageView;

/// Color state of a view or output (stub for ClutterColorState).
///
/// Real Mutter derives the view color state from the output color state and a
/// "force linear blending" debug flag. Modeled here as an opaque handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorState {
    pub id: u64,
    pub linear_blending: bool,
}

/// A color-managed device backing an output (stub for MetaColorDevice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorDevice {
    pub id: u64,
    pub color_state: ColorState,
}

/// A renderer view: the stage view for one CRTC / logical monitor.
#[derive(Debug)]
pub struct RendererView {
    /// Base Clutter/Mutter stage-view state.
    pub stage_view: StageView,
    /// Opaque backend handle (MetaBackend pointer in Mutter). Stubbed as id.
    pub backend_id: u64,
    /// The CRTC this view scans out to, if assigned.
    crtc_id: Option<u64>,
    /// The color device for this view's output, if color managed.
    color_device: Option<ColorDevice>,
    /// The blending color state computed for this view.
    view_color_state: Option<ColorState>,
    /// The output's color state, tracked separately from the view's.
    output_color_state: Option<ColorState>,
}

impl RendererView {
    pub fn new(backend_id: u64, crtc_id: Option<u64>, color_device: Option<ColorDevice>) -> Self {
        let mut view = RendererView {
            stage_view: StageView::new(),
            backend_id,
            crtc_id,
            color_device,
            view_color_state: None,
            output_color_state: None,
        };

        // Faithful to meta_renderer_view_constructed: if a color device is
        // present, compute color states immediately and (in Mutter) subscribe to
        // its "color-state-changed" signal.
        if view.color_device.is_some() {
            view.set_color_states(false);
        }

        view
    }

    /// The CRTC this view renders to.
    pub fn get_crtc_id(&self) -> Option<u64> {
        self.crtc_id
    }

    /// Recompute and apply the view and output color states.
    ///
    /// Faithful to set_color_states: the view uses the output's blending color
    /// state (optionally forced to linear), while the output color state is
    /// tracked separately. `force_linear` corresponds to the debug-control flag.
    pub fn set_color_states(&mut self, force_linear: bool) {
        let Some(color_device) = self.color_device else {
            return;
        };

        let output_color_state = color_device.color_state;
        let view_color_state = ColorState {
            id: output_color_state.id,
            // clutter_color_state_get_blending(output, force_linear) - stubbed
            // to reflect the requested blending linearity.
            linear_blending: force_linear || output_color_state.linear_blending,
        };

        self.view_color_state = Some(view_color_state);
        self.output_color_state = Some(output_color_state);
    }

    /// Handle a color-state-changed notification from the color device.
    /// Mirrors on_color_state_changed.
    pub fn on_color_state_changed(&mut self, new_state: ColorState) {
        if let Some(color_device) = &mut self.color_device {
            color_device.color_state = new_state;
        }
        self.set_color_states(false);
    }

    pub fn get_view_color_state(&self) -> Option<ColorState> {
        self.view_color_state
    }

    pub fn get_output_color_state(&self) -> Option<ColorState> {
        self.output_color_state
    }
}
