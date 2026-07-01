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

// --- Wired-in ported modules (previously undeclared) ---
pub mod activation;
pub mod client;
pub mod color_management;
pub mod color_representation;
pub mod commit_timing;
pub mod cursor_shape;
pub mod cursor_surface;
pub mod data_device_primary;
pub mod data_offer_primary;
pub mod data_source_primary;
pub mod dma_buf;
pub mod dnd_surface;
pub mod drm_lease;
pub mod fifo;
pub mod filter_manager;
pub mod gtk_shell;
pub mod idle_inhibit;
pub mod inhibit_shortcuts;
pub mod inhibit_shortcuts_dialog;
pub mod legacy_xdg_foreign;
pub mod linux_drm_syncobj;
pub mod pointer_confinement_wayland;
pub mod pointer_constraints;
pub mod pointer_gesture_hold;
pub mod pointer_gesture_pinch;
pub mod pointer_gesture_swipe;
pub mod pointer_gestures;
pub mod pointer_lock_wayland;
pub mod pointer_warp;
pub mod selection_source_wayland;
pub mod single_pixel_buffer;
pub mod tablet;
pub mod tablet_cursor_surface;
pub mod tablet_manager;
pub mod tablet_pad;
pub mod tablet_pad_dial;
pub mod tablet_pad_group;
pub mod tablet_pad_ring;
pub mod tablet_pad_strip;
pub mod tablet_seat;
pub mod tablet_tool;
pub mod text_input;
pub mod toplevel_drag;
pub mod wayland;
pub mod x11_interop;
pub mod xdg_dialog;
pub mod xdg_foreign;
pub mod xdg_session;
pub mod xdg_session_manager;
pub mod xdg_session_state;
pub mod xdg_toplevel_tag;
pub mod xwayland;
pub mod xwayland_dnd;
pub mod xwayland_grab_keyboard;
pub mod xwayland_surface;
