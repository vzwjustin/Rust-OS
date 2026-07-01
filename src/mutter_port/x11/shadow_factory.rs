//! X11 window shadow rendering factory.
//!
//! Ported from GNOME Mutter's src/x11/meta-shadow-factory.c/.h.
//! Generates and caches drop shadows for windows with various shapes and sizes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-shadow-factory.c

/// Represents a precomputed window shadow.
#[derive(Debug, Clone)]
pub struct Shadow {
    pub shadow_id: u64,

    /// Shadow extent in pixels (how far from the window edge).
    pub extent: u32,

    /// Shadow blur radius.
    pub blur_radius: u32,

    /// Offsets for shadow layers.
    pub offset_x: i32,
    pub offset_y: i32,

    /// Opacity of shadow (0.0 to 1.0).
    pub opacity: f32,

    /// Cached shadow data (raw pixel data or region).
    pub cached_data: Option<alloc::vec::Vec<u8>>,
}

impl Shadow {
    /// Create a new shadow with default parameters.
    pub fn new(shadow_id: u64) -> Self {
        Self {
            shadow_id,
            extent: 10,
            blur_radius: 5,
            offset_x: 0,
            offset_y: 0,
            opacity: 0.5,
            cached_data: None,
        }
    }

    /// Set shadow parameters.
    /// # TODO: port logic from shadow parameter configuration
    pub fn set_parameters(
        &mut self,
        extent: u32,
        blur_radius: u32,
        offset_x: i32,
        offset_y: i32,
        opacity: f32,
    ) {
        self.extent = extent;
        self.blur_radius = blur_radius;
        self.offset_x = offset_x;
        self.offset_y = offset_y;
        self.opacity = opacity;
        self.cached_data = None; // Invalidate cache
    }

    /// Generate shadow pixel data for a window of given dimensions.
    /// # TODO: port logic from meta_shadow_factory_get_shadow()
    pub fn generate_for_size(&mut self, width: u32, height: u32) {
        // TODO: generate shadow texture using Gaussian blur
        self.cached_data = Some(alloc::vec::Vec::new());
    }

    /// Get the shadow data for rendering.
    pub fn get_data(&self) -> Option<&[u8]> {
        self.cached_data.as_deref()
    }
}

/// Factory for creating and caching shadows.
pub struct MetaShadowFactory {
    shadows: alloc::collections::BTreeMap<u64, Shadow>,
    next_shadow_id: u64,
}

impl MetaShadowFactory {
    /// Create a new shadow factory.
    /// # TODO: port logic from meta_shadow_factory_new()
    pub fn new() -> Self {
        Self {
            shadows: alloc::collections::BTreeMap::new(),
            next_shadow_id: 1,
        }
    }

    /// Create a new shadow in this factory.
    pub fn create_shadow(&mut self) -> u64 {
        let shadow_id = self.next_shadow_id;
        self.next_shadow_id += 1;

        let shadow = Shadow::new(shadow_id);
        self.shadows.insert(shadow_id, shadow);
        shadow_id
    }

    /// Get a shadow by ID.
    pub fn get_shadow(&self, shadow_id: u64) -> Option<&Shadow> {
        self.shadows.get(&shadow_id)
    }

    /// Get a mutable shadow by ID.
    pub fn get_shadow_mut(&mut self, shadow_id: u64) -> Option<&mut Shadow> {
        self.shadows.get_mut(&shadow_id)
    }

    /// Release a shadow.
    pub fn release_shadow(&mut self, shadow_id: u64) -> bool {
        self.shadows.remove(&shadow_id).is_some()
    }

    /// Clear all cached shadows.
    pub fn clear_cache(&mut self) {
        for shadow in self.shadows.values_mut() {
            shadow.cached_data = None;
        }
    }
}

impl Default for MetaShadowFactory {
    fn default() -> Self {
        Self::new()
    }
}
