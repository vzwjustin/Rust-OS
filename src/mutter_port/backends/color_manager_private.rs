//! Color Manager Private ported from GNOME Mutter's src/backends/
//!
//! Private implementation details for MetaColorManager. Contains the GObject class
//! structure and internal-only function signatures for color profile management.
//! Handles colord device discovery, LCMS color context, and per-monitor color stores.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-color-manager-private.h

/// Vtable struct for _MetaColorManagerClass.
/// In C, contains GObjectClass parent_class. This is an opaque GObject vtable;
/// method callbacks for colord integration and LCMS context are backend-dependent.
/// Documented as empty per no_std constraints (GObject introspection unavailable).
pub struct MetaColorManagerClass {
    // GObjectClass parent_class (opaque, omitted in no_std)
    // No methods exposed in no_std stub
}
