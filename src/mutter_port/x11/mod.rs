//! GNOME Mutter X11 backend port.
//!
//! Ported from GNOME Mutter's src/x11/ directory (~39 .c/.h files).
//! This module provides X11 protocol support including:
//! - Window management (ICCCM/EWMH)
//! - Event handling and dispatch
//! - Selection/clipboard handling
//! - Window decorations and shapes
//! - X extensions (XSync, Shape, Damage, Composite, XFixes)
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/

pub mod atoms;
pub mod display;
pub mod event_source;
pub mod events;
pub mod frame;
pub mod group;
pub mod properties;
pub mod selection;
pub mod shadow_factory;
pub mod shape;
pub mod stack;
pub mod startup_notification;
pub mod sync_counter;
pub mod window;

// Re-export key types for convenience
pub use atoms::{Atom, AtomNames};
pub use display::{MetaX11Display, XWindow};
pub use event_source::{EventSourceId, MetaX11EventSource};
pub use events::XEvent;
pub use frame::{FrameId, MetaX11Frame};
pub use group::{GroupId, MetaX11Group};
pub use properties::{
    delete_property, get_net_wm_name, get_net_wm_icon_geometry, get_property, get_size_hints,
    get_wm_hints, set_property, GroupPropertyHook, PropertyType, PropertyValue, SizeHints,
    WindowPropertyHook, WmHints,
};
pub use selection::{
    MetaSelectionSourceX11, MetaX11SelectionInputStream, MetaX11SelectionOutputStream,
    SelectionType,
};
pub use shadow_factory::{MetaShadowFactory, Shadow};
pub use shape::WindowShape;
pub use stack::MetaX11Stack;
pub use startup_notification::{MetaX11StartupNotification, StartupSequence};
pub use sync_counter::{SyncCounter, SyncRequestHandler};
pub use window::MetaWindowX11;
