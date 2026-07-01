//! Port of mutter's `mtk` (Mutter ToolKit) primitives: geometry types,
//! region algebra, and monitor transform/utility helpers.
//!
//! Not yet wired into the kernel build (no `mod mutter_port;` in main.rs/lib.rs).

pub mod monitor_transform;
pub mod rectangle;
pub mod region;
pub mod time_utils;
pub mod utils;
