//! Monitor/output/rendering backend primitives ported from GNOME Mutter's
//! `src/backends/` (excluding the native DRM/KMS backend, which lives in
//! `native`, and Wayland-specific code, which lives in `mutter_port::wayland`).

pub mod native;

pub mod barrier;
pub mod connector;
pub mod crtc;
pub mod crtc_mode;
pub mod dbus_access_checker;
pub mod dbus_utils;
pub mod edid_parse;
pub mod fd_source;
pub mod gpu;
pub mod idle_manager;
pub mod idle_monitor;
pub mod keymap_description;
pub mod keymap_utils;
pub mod logical_monitor;
pub mod orientation_manager;
pub mod output;
pub mod pointer_constraint;
pub mod renderer;
pub mod renderer_view;
pub mod settings;
pub mod sprite;
pub mod stage;
pub mod stage_view;
pub mod udev;
pub mod viewport_info;
