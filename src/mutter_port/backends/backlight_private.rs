//! Backlight Private ported from GNOME Mutter's src/backends/
//!
//! Private GObject class structure for MetaBacklight. Defines vfunc hooks
//! for async brightness setting via hardware-specific backends.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-private.h

/// Placeholder for _MetaBacklightClass structure.
/// Real C structure has GObjectClass parent_class and two function pointers:
/// - set_brightness (async start)
/// - set_brightness_finish (async complete, returns int result)
pub struct MetaBacklightClass {
    // GObjectClass parent_class;
    // set_brightness and set_brightness_finish vfuncs would go here
}
