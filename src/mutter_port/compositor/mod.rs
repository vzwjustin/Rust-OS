//! Ported from GNOME Mutter's `src/compositor/` — actor/rendering
//! layer built on Clutter/Cogl primitives.

pub mod background;
pub mod background_actor;
pub mod background_content;
pub mod background_group;
pub mod background_image;
pub mod clutter_utils;
pub mod cogl_utils;
pub mod compositor;
pub mod compositor_native;
pub mod compositor_server;
pub mod compositor_view;
pub mod compositor_view_native;
pub mod cullable;
pub mod dnd;
pub mod dnd_actor;
pub mod edge_resistance;
pub mod feedback_actor;
pub mod later;
pub mod module;
pub mod multi_texture;
pub mod multi_texture_format;
pub mod plugin;
pub mod plugin_manager;
pub mod shaped_texture;
pub mod surface_actor;
pub mod surface_actor_wayland;
pub mod texture_mipmap;
pub mod window_actor;
pub mod window_actor_wayland;
pub mod window_actor_x11;
pub mod window_drag;
pub mod window_group;
