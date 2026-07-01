//! Port of GNOME mutter's `clutter/clutter-color-manager.{c,h}`.
//! Manages caching and deduplication of ColorState instances by parameters.
//!
//! Skipped:
//! - GObject class machinery (G_DECLARE_FINAL_TYPE, _init/_class_init).
//! - ClutterContext (GObject compositor singleton).
//! - Signal emission and property notifications.
//! - Snippet caching (GPU/Cogl shader pipeline integration).
//! - id_counter (GObject registration and type-system IDs).

use super::color_state::{
    colorimetry_equal, eotf_equal, luminance_equal, Colorimetry, Colorspace, Eotf, Luminance,
    TransferFunction, SDR_DEFAULT_LUMINANCE,
};
use alloc::vec::Vec;

/// A color state wrapping colorimetry (primaries/colorspace), EOTF
/// (transfer function), and luminance parameters.
///
/// Mirrors the essential data of `ClutterColorState` (minus GObject boilerplate
/// and Cogl pipeline machinery). All fields are `Copy` to enable efficient
/// caching without heap allocation or reference counting.
#[derive(Clone, Copy, Debug)]
pub struct ColorState {
    pub colorimetry: Colorimetry,
    pub eotf: Eotf,
    pub luminance: Luminance,
}

impl ColorState {
    /// Creates a new `ColorState` with explicit parameters.
    pub fn new(colorimetry: Colorimetry, eotf: Eotf, luminance: Luminance) -> Self {
        ColorState {
            colorimetry,
            eotf,
            luminance,
        }
    }

    /// Returns the default sRGB color state (Colorspace::Srgb + Gamma22 transfer
    /// function + SDR luminance).
    ///
    /// Mirrors the default constructed by `clutter_color_manager_get_default_color_state`.
    pub fn default() -> Self {
        ColorState {
            colorimetry: Colorimetry::Colorspace(Colorspace::Srgb),
            eotf: Eotf::Named(TransferFunction::Gamma22),
            luminance: SDR_DEFAULT_LUMINANCE,
        }
    }

    /// Returns whether this `ColorState` has parameters equal to another,
    /// using approximate comparison for floating-point fields.
    fn params_equal(&self, other: &ColorState) -> bool {
        colorimetry_equal(&self.colorimetry, &other.colorimetry)
            && eotf_equal(&self.eotf, &other.eotf)
            && luminance_equal(&self.luminance, &other.luminance)
    }
}

/// Manages a cache of `ColorState` instances, deduplicating by parameters.
///
/// Mirrors `ClutterColorManager` (minus GObject machinery, Cogl snippets, and
/// context references). Uses a small `Vec` for the cache, suitable for the
/// typical case of 1-5 color states per display/context.
pub struct ColorManager {
    default_color_state: Option<ColorState>,
    cache: Vec<ColorState>,
}

impl ColorManager {
    /// Creates a new, empty `ColorManager`.
    pub fn new() -> Self {
        ColorManager {
            default_color_state: None,
            cache: Vec::new(),
        }
    }

    /// Returns the cached default sRGB color state, creating it on first access.
    ///
    /// Mirrors `clutter_color_manager_get_default_color_state`.
    pub fn get_default(&mut self) -> ColorState {
        if self.default_color_state.is_none() {
            self.default_color_state = Some(ColorState::default());
        }
        self.default_color_state.unwrap()
    }

    /// Finds or creates a `ColorState` with the given parameters, deduplicating
    /// on return.
    ///
    /// Linearly scans the cache for a `ColorState` with parameters equal to
    /// the given ones (via approximate comparison). If found, returns it. If not,
    /// appends it to the cache and returns the newly cached instance.
    pub fn find_or_create(
        &mut self,
        colorimetry: Colorimetry,
        eotf: Eotf,
        luminance: Luminance,
    ) -> ColorState {
        let params = ColorState::new(colorimetry, eotf, luminance);

        for state in &self.cache {
            if state.params_equal(&params) {
                return *state;
            }
        }

        self.cache.push(params);
        params
    }
}

impl Default for ColorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_state_default() {
        let state = ColorState::default();
        assert_eq!(state.colorimetry, Colorimetry::Colorspace(Colorspace::Srgb));
        assert_eq!(state.eotf, Eotf::Named(TransferFunction::Gamma22));
        assert_eq!(state.luminance, SDR_DEFAULT_LUMINANCE);
    }

    #[test]
    fn test_color_state_params_equal_same() {
        let s1 = ColorState::default();
        let s2 = ColorState::default();
        assert!(s1.params_equal(&s2));
    }

    #[test]
    fn test_color_manager_get_default_cached() {
        let mut mgr = ColorManager::new();
        let state1 = mgr.get_default();
        let state2 = mgr.get_default();
        assert_eq!(state1.colorimetry, state2.colorimetry);
        assert_eq!(state1.eotf, state2.eotf);
    }

    #[test]
    fn test_color_manager_find_or_create_new() {
        let mut mgr = ColorManager::new();
        let state = mgr.find_or_create(
            Colorimetry::Colorspace(Colorspace::Bt2020),
            Eotf::Named(TransferFunction::Pq),
            SDR_DEFAULT_LUMINANCE,
        );
        assert_eq!(
            state.colorimetry,
            Colorimetry::Colorspace(Colorspace::Bt2020)
        );
        assert_eq!(state.eotf, Eotf::Named(TransferFunction::Pq));
    }

    #[test]
    fn test_color_manager_find_or_create_dedup() {
        let mut mgr = ColorManager::new();
        let state1 = mgr.find_or_create(
            Colorimetry::Colorspace(Colorspace::Srgb),
            Eotf::Named(TransferFunction::Linear),
            SDR_DEFAULT_LUMINANCE,
        );
        let state2 = mgr.find_or_create(
            Colorimetry::Colorspace(Colorspace::Srgb),
            Eotf::Named(TransferFunction::Linear),
            SDR_DEFAULT_LUMINANCE,
        );
        assert_eq!(state1.colorimetry, state2.colorimetry);
        assert_eq!(state1.eotf, state2.eotf);
        assert_eq!(mgr.cache.len(), 1);
    }
}
