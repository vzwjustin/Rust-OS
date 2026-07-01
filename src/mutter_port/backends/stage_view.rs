//! Stage view ported from GNOME Mutter's src/backends/meta-stage-view.c
//!
//! A stage view is Mutter's specialization of a Clutter stage view. It tracks
//! per-view damage history, presentation/frame callbacks, and a cursor-overlay
//! inhibit count.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-view.c

/// Paint flags controlling how a view is painted (mirrors ClutterPaintFlag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintFlag {
    None,
    NoCursors,
}

/// Frame timing info reported on presentation (subset of ClutterFrameInfo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameInfo {
    pub global_frame_counter: i64,
    pub view_frame_counter: i64,
    pub refresh_rate: u32,
    /// Presentation timestamp, microseconds (monotonic).
    pub presentation_time_us: i64,
    pub sequence: i64,
}

/// A Mutter stage view.
///
/// The damage history, frame-callback closure, and presentation idle source
/// from the C original are hardware/GSource concepts and are represented as
/// simple state here; the actual Cogl onscreen and GLib idle wiring is stubbed.
#[derive(Debug)]
pub struct StageView {
    /// Damage history is tracked in Mutter via ClutterDamageHistory; here it is
    /// modeled as a queue of damaged frame regions counted for bookkeeping.
    /// Stub: real region math lives in Clutter.
    damage_history_len: usize,
    /// Whether a Cogl frame callback is installed on the onscreen framebuffer.
    /// Stub: no real Cogl onscreen in-kernel.
    frame_cb_installed: bool,
    /// Whether a notify-presented idle source is pending.
    notify_presented_pending: bool,
    /// Cursor-overlay inhibit count; while > 0 cursors are not painted.
    inhibit_cursor_overlay_count: u32,
}

impl StageView {
    pub fn new() -> Self {
        StageView {
            damage_history_len: 0,
            frame_cb_installed: false,
            notify_presented_pending: false,
            inhibit_cursor_overlay_count: 0,
        }
    }

    /// Called at construction to install the Cogl frame callback if the view is
    /// backed by an onscreen framebuffer. Faithful to meta_stage_view_constructed.
    pub fn install_frame_callback(&mut self) {
        // cogl_onscreen_add_frame_callback(framebuffer, frame_cb, ...) - stubbed.
        self.frame_cb_installed = true;
    }

    /// Default paint flags for this view: cursors are suppressed while inhibited.
    /// Faithful port of meta_stage_view_get_default_paint_flags.
    pub fn get_default_paint_flags(&self) -> PaintFlag {
        if self.inhibit_cursor_overlay_count > 0 {
            PaintFlag::NoCursors
        } else {
            PaintFlag::None
        }
    }

    /// Number of entries in the (stubbed) damage history.
    pub fn get_damage_history_len(&self) -> usize {
        self.damage_history_len
    }

    /// Record a damaged frame in the history. Stub for ClutterDamageHistory.
    pub fn record_damage(&mut self) {
        self.damage_history_len = self.damage_history_len.saturating_add(1);
    }

    /// Schedule a fake swap: notify presentation on the next idle.
    ///
    /// Faithful to meta_stage_view_perform_fake_swap in structure — it builds a
    /// synthetic frame info and defers notify-presented. The GLib high-priority
    /// idle source is stubbed as a pending flag; call `flush_fake_swap` to run it.
    pub fn perform_fake_swap(
        &mut self,
        global_frame_counter: i64,
        view_frame_counter: i64,
        refresh_rate: u32,
        monotonic_time_us: i64,
    ) -> FrameInfo {
        debug_assert!(!self.notify_presented_pending);
        self.notify_presented_pending = true;

        FrameInfo {
            global_frame_counter,
            view_frame_counter,
            refresh_rate,
            presentation_time_us: monotonic_time_us,
            sequence: 0,
        }
    }

    /// Run the deferred notify-presented from a fake swap.
    /// Mirrors notify_presented_idle clearing its handle id.
    pub fn flush_fake_swap(&mut self) -> bool {
        if self.notify_presented_pending {
            self.notify_presented_pending = false;
            // clutter_stage_view_notify_presented(...) - stubbed.
            true
        } else {
            false
        }
    }

    /// Inhibit cursor-overlay painting on this view.
    pub fn inhibit_cursor_overlay(&mut self) {
        self.inhibit_cursor_overlay_count += 1;
    }

    /// Uninhibit cursor-overlay painting (must be currently inhibited).
    pub fn uninhibit_cursor_overlay(&mut self) {
        debug_assert!(self.inhibit_cursor_overlay_count > 0);
        self.inhibit_cursor_overlay_count -= 1;
    }

    /// Whether cursor-overlay painting is currently inhibited.
    pub fn is_cursor_overlay_inhibited(&self) -> bool {
        self.inhibit_cursor_overlay_count > 0
    }
}

impl Default for StageView {
    fn default() -> Self {
        Self::new()
    }
}
