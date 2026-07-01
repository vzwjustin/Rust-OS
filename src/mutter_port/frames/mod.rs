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
pub fn should_monitor_color_scheme() -> bool {
    // TODO: port meta_frames_client_should_monitor_color_scheme
    false
}
