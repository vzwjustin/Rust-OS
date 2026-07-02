//! Backend Private ported from GNOME Mutter's src/backends/
//!
//! Private backend interface with class vtable for platform-specific implementations.
//! Defines the MetaBackendClass virtual method table and core backend accessor functions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backend-private.h

use alloc::boxed::Box;
use alloc::string::String;

/// Default XKB rules file.
pub const DEFAULT_XKB_RULES_FILE: &str = "evdev";

/// Default XKB model.
pub const DEFAULT_XKB_MODEL: &str = "pc105+inet";

/// Opaque placeholder structs for Clutter/GLib integration.
pub struct ClutterBackend;
pub struct ClutterContext;
pub struct ClutterSeat;
pub struct ClutterSprite;
pub struct ClutterEvent;
pub struct ClutterCursorType;
pub struct ClutterCursor;
pub struct GError;
pub struct GList;
pub struct GAsyncResult;
pub struct GTask;
pub struct GCancellable;
pub struct GObject;
pub struct MetaBackendCapabilities;
pub struct MetaA11yManager;
pub struct MetaCursorTracker;
pub struct MetaCursorRenderer;
pub struct MetaEgl;
pub struct MetaInputSettings;
pub struct MetaInputMapper;
pub struct MetaKeymapDescription;
pub struct MetaKeymapDescriptionOwner;
pub struct MetaPointerConstraint;
pub struct MetaHwCursorInhibitor;
pub struct WacomDeviceDatabase;

/// Type for xkb_layout_index_t.
pub type XkbLayoutIndex = u32;

/// Type for GAsyncReadyCallback.
pub type GAsyncReadyCallback = *const ();

/// Type for gpointer (void*).
pub type GPointer = *const ();

// Stub function declarations (not yet fully ported)
// In no_std, we avoid full function bodies without proper FFI bindings
