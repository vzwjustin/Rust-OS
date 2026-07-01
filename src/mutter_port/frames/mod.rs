//! Mutter frames module - window frame decoration and rendering.

pub mod frame;
pub mod frame_content;
pub mod frame_header;
pub mod window_tracker;

pub use frame::Frame;
pub use frame_content::FrameContent;
pub use frame_header::FrameHeader;
pub use window_tracker::WindowTracker;

/// Check if the system should monitor color scheme for frame decorations.
/// A full implementation would check the GSettings "monitors-color-scheme"
/// key. Without GSettings, returns false (no color scheme monitoring).
pub fn should_monitor_color_scheme() -> bool {
    // Would read the GSettings key for interface color-scheme preference.
    // Without GSettings, color scheme monitoring is disabled.
    false
}
