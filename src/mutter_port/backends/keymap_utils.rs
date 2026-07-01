//! keymap_utils - helpers for building an xkbcommon context.
//!
//! Ported from GNOME Mutter's src/backends/meta-keymap-utils.c. libxkbcommon and
//! its include-path handling are not available in the kernel, so the actual
//! context creation is stubbed; the search-path assembly logic (XDG dir then the
//! default includes) is preserved as data.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-utils.c

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A stub for `struct xkb_context`. In real Mutter this owns libxkbcommon state;
/// here we only track the assembled include search paths.
#[derive(Debug, Clone, Default)]
pub struct XkbContext {
    /// Include search paths in append order (XDG dir first, then defaults).
    pub include_paths: Vec<String>,
}

/// Default xkb include paths appended by `xkb_context_include_path_append_default`.
/// Kept as a faithful stand-in for the libxkbcommon compiled-in defaults.
const DEFAULT_INCLUDE_PATHS: &[&str] = &["/usr/share/X11/xkb", "/etc/xkb"];

/// Build an xkb context. Mirrors `meta_create_xkb_context`.
///
/// Starts with an empty include set, appends `$XDG_CONFIG_HOME/xkb` (or
/// `$HOME/.config/xkb`), then appends the default search paths.
///
/// `xdg_config_home` and `home` stand in for the g_getenv() lookups, which are
/// unavailable in the kernel.
pub fn create_xkb_context(xdg_config_home: Option<&str>, home: Option<&str>) -> XkbContext {
    let mut ctx = XkbContext::default();

    let xdg = if let Some(env) = xdg_config_home {
        Some(format!("{}/xkb", env))
    } else {
        home.map(|env| format!("{}/.config/xkb", env))
    };

    if let Some(path) = xdg {
        ctx.include_paths.push(path);
    }

    for p in DEFAULT_INCLUDE_PATHS {
        ctx.include_paths.push(String::from(*p));
    }

    ctx
}
