//! GNOME Mutter Wayland protocol layer
//!
//! This module ports GNOME Mutter's `src/wayland/` subsystem to no_std Rust.
//! Each submodule models the corresponding `meta-wayland-*.c` (or `meta-*-wayland.c`)
//! source file: core compositor, surfaces and roles, buffers, input devices,
//! shell protocols, data-device/clipboard/DnD, outputs, and protocol extensions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/tree/main/src/wayland

pub mod actor_surface;
pub mod buffer;
pub mod cursor_wayland;
pub mod data_device;
pub mod data_offer;
pub mod data_source;
pub mod fixes;
pub mod fractional_scale;
pub mod input;
pub mod input_device;
pub mod keyboard;
pub mod output;
pub mod outputs;
pub mod pointer;
pub mod popup;
pub mod presentation_time;
pub mod region;
pub mod seat;
pub mod shell_surface;
pub mod subsurface;
pub mod surface;
pub mod system_bell;
pub mod touch;
pub mod transaction;
pub mod viewporter;
pub mod wayland_root;
pub mod window_configuration;
pub mod window_wayland;
pub mod window_xwayland;
pub mod xdg_shell;
