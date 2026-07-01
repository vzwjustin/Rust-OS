//! MetaCursorRenderer ported from GNOME Mutter's src/core/meta-cursor-renderer.c
//!
//! MetaCursorRenderer is responsible for drawing the cursor. It decides
//! whether to use the hardware cursor (via DRM/KMS cursor planes) or fall
//! back to a software cursor (rendered into the framebuffer by the compositor).
//!
//! In Mutter this is an abstract GObject with backend-specific subclasses
//! (MetaCursorRendererNative, etc.). Here it is a concrete struct with a
//! `CursorRendererBackend` trait that the native backend implements.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-cursor-renderer.c

use alloc::vec::Vec;

use super::cursor_sprite::CursorSprite;
use super::cursor_tracker::CursorRole;

/// The cursor rendering mode. Mirrors the hardware/software decision in
/// meta_cursor_renderer_update_cursor().
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorRenderMode {
    /// Hardware cursor plane is being used.
    Hardware,
    /// Software cursor (compositor-rendered).
    Software,
    /// Cursor is hidden.
    Hidden,
}

/// Backend hooks for hardware cursor operations. The native backend
/// implements this to interact with DRM/KMS cursor planes.
pub trait CursorRendererBackend {
    /// Whether a hardware cursor can be used for the given sprite.
    /// Mirrors meta_cursor_renderer_native_can_draw_cursor().
    fn can_use_hardware_cursor(&self, sprite: &CursorSprite) -> bool;

    /// Set the hardware cursor buffer. Mirrors
    /// meta_cursor_renderer_native_set_cursor().
    fn set_hardware_cursor(&mut self, sprite: &CursorSprite, x: i32, y: i32);

    /// Disable the hardware cursor plane. Mirrors
    /// meta_cursor_renderer_native_unset_cursor().
    fn unset_hardware_cursor(&mut self);

    /// Whether the backend supports cursor scaling.
    fn supports_scaling(&self) -> bool {
        false
    }
}

/// The cursor renderer. Mirrors MetaCursorRenderer.
#[derive(Debug)]
pub struct MetaCursorRenderer {
    /// Current rendering mode.
    mode: CursorRenderMode,
    /// Current cursor sprite (if any).
    current_sprite: Option<CursorSprite>,
    /// Cursor position in stage coordinates.
    position: (i32, i32),
    /// The cursor role (pointer vs tablet tool).
    role: CursorRole,
    /// Whether the renderer is enabled.
    enabled: bool,
    /// Whether the cursor needs re-rendering.
    needs_render: bool,
    /// Rectangles of the cursor that need to be redrawn (for software mode).
    damage_rects: Vec<CursorRect>,
}

/// A damage rectangle in stage coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl CursorRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        CursorRect {
            x,
            y,
            width,
            height,
        }
    }
}

impl MetaCursorRenderer {
    /// Create a new cursor renderer. Mirrors meta_cursor_renderer_new().
    pub fn new() -> Self {
        MetaCursorRenderer {
            mode: CursorRenderMode::Hidden,
            current_sprite: None,
            position: (0, 0),
            role: CursorRole::Pointer,
            enabled: true,
            needs_render: false,
            damage_rects: Vec::new(),
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn mode(&self) -> CursorRenderMode {
        self.mode
    }

    pub fn current_sprite(&self) -> Option<&CursorSprite> {
        self.current_sprite.as_ref()
    }

    pub fn position(&self) -> (i32, i32) {
        self.position
    }

    pub fn role(&self) -> CursorRole {
        self.role
    }

    pub fn set_role(&mut self, role: CursorRole) {
        self.role = role;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.needs_render = true;
        }
    }

    // ── Cursor updates ────────────────────────────────────────────────

    /// Set the current cursor sprite. Mirrors
    /// meta_cursor_renderer_set_cursor().
    ///
    /// If the sprite changed, marks a render as needed and computes damage.
    pub fn set_cursor(&mut self, sprite: Option<CursorSprite>) {
        let changed = match (&self.current_sprite, &sprite) {
            (None, None) => false,
            (Some(a), Some(b)) => a.texture_id() != b.texture_id(),
            _ => true,
        };

        if changed {
            // Damage the old cursor rect.
            let old_damage = self
                .current_sprite
                .as_ref()
                .map(|old| Self::cursor_rect_for(self.position, old));
            if let Some(rect) = old_damage {
                self.damage_rects.push(rect);
            }
            self.current_sprite = sprite;
            self.needs_render = true;
            // Damage the new cursor rect.
            let new_damage = self
                .current_sprite
                .as_ref()
                .map(|new| Self::cursor_rect_for(self.position, new));
            if let Some(rect) = new_damage {
                self.damage_rects.push(rect);
            }
        }
    }

    /// Update the cursor position. Mirrors
    /// meta_cursor_renderer_update_position().
    pub fn set_position(&mut self, x: i32, y: i32) {
        if self.position != (x, y) {
            // Damage old position.
            let old_damage = self
                .current_sprite
                .as_ref()
                .map(|sprite| Self::cursor_rect_for(self.position, sprite));
            if let Some(rect) = old_damage {
                self.damage_rects.push(rect);
            }
            self.position = (x, y);
            self.needs_render = true;
            // Damage new position.
            let new_damage = self
                .current_sprite
                .as_ref()
                .map(|sprite| Self::cursor_rect_for(self.position, sprite));
            if let Some(rect) = new_damage {
                self.damage_rects.push(rect);
            }
        }
    }

    // ── Rendering ─────────────────────────────────────────────────────

    /// Determine whether to use hardware or software cursor. Mirrors
    /// meta_cursor_renderer_update_cursor() which calls the backend's
    /// can_draw_cursor() vfunc.
    ///
    /// Returns the computed render mode. The backend (if provided) is
    /// consulted for hardware cursor support.
    pub fn update_cursor(&mut self, mut backend: Option<&mut dyn CursorRendererBackend>) {
        if !self.enabled {
            self.mode = CursorRenderMode::Hidden;
            if let Some(b) = backend.as_deref_mut() {
                b.unset_hardware_cursor();
            }
            return;
        }

        let Some(sprite) = self.current_sprite.as_ref() else {
            self.mode = CursorRenderMode::Hidden;
            if let Some(b) = backend.as_deref_mut() {
                b.unset_hardware_cursor();
            }
            return;
        };
        let can_hw = match backend.as_deref_mut() {
            Some(b) => b.can_use_hardware_cursor(sprite),
            None => false,
        };

        if can_hw {
            self.mode = CursorRenderMode::Hardware;
            if let Some(b) = backend.as_deref_mut() {
                let (x, y) = self.position;
                b.set_hardware_cursor(sprite, x, y);
            }
        } else {
            self.mode = CursorRenderMode::Software;
            if let Some(b) = backend.as_deref_mut() {
                b.unset_hardware_cursor();
            }
            self.needs_render = true;
        }
    }

    /// Whether the renderer needs to draw the cursor (software mode).
    pub fn needs_render(&self) -> bool {
        self.needs_render && self.mode == CursorRenderMode::Software
    }

    /// Clear the render-needed flag after the compositor has drawn.
    pub fn clear_needs_render(&mut self) {
        self.needs_render = false;
    }

    /// Drain damage rectangles. The compositor uses these to know which
    /// regions of the framebuffer to repaint.
    pub fn take_damage(&mut self) -> Vec<CursorRect> {
        core::mem::take(&mut self.damage_rects)
    }

    // ── Internal helpers ──────────────────────────────────────────────

    fn cursor_rect_for(position: (i32, i32), sprite: &CursorSprite) -> CursorRect {
        let (hx, hy) = sprite.scaled_hotspot();
        let (w, h) = sprite.scaled_dimensions();
        let (x, y) = position;
        CursorRect::new(x - hx, y - hy, w as i32, h as i32)
    }

    fn damage_old_cursor(&mut self, sprite: &CursorSprite) {
        self.damage_rects
            .push(Self::cursor_rect_for(self.position, sprite));
    }

    fn damage_new_cursor(&mut self, sprite: &CursorSprite) {
        self.damage_rects
            .push(Self::cursor_rect_for(self.position, sprite));
    }

    /// Get the cursor's bounding rect at the current position.
    pub fn cursor_rect(&self) -> Option<CursorRect> {
        let sprite = self.current_sprite.as_ref()?;
        let (hx, hy) = sprite.scaled_hotspot();
        let (w, h) = sprite.scaled_dimensions();
        let (x, y) = self.position;
        Some(CursorRect::new(x - hx, y - hy, w as i32, h as i32))
    }
}

impl Default for MetaCursorRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend {
        can_hw: bool,
        set_called: bool,
        unset_called: bool,
    }

    impl CursorRendererBackend for MockBackend {
        fn can_use_hardware_cursor(&self, _sprite: &CursorSprite) -> bool {
            self.can_hw
        }
        fn set_hardware_cursor(&mut self, _sprite: &CursorSprite, _x: i32, _y: i32) {
            self.set_called = true;
        }
        fn unset_hardware_cursor(&mut self) {
            self.unset_called = true;
        }
    }

    #[test]
    fn test_default_state() {
        let renderer = MetaCursorRenderer::new();
        assert_eq!(renderer.mode(), CursorRenderMode::Hidden);
        assert!(renderer.current_sprite().is_none());
        assert!(renderer.is_enabled());
    }

    #[test]
    fn test_set_cursor() {
        let mut renderer = MetaCursorRenderer::new();
        let sprite = CursorSprite::new(1, 32, 32);
        renderer.set_cursor(Some(sprite));

        assert!(renderer.current_sprite().is_some());
        assert!(renderer.needs_render());
        assert!(!renderer.take_damage().is_empty());
    }

    #[test]
    fn test_position_change() {
        let mut renderer = MetaCursorRenderer::new();
        renderer.set_cursor(Some(CursorSprite::new(1, 32, 32)));
        renderer.clear_needs_render();
        let _ = renderer.take_damage();

        renderer.set_position(100, 200);
        assert_eq!(renderer.position(), (100, 200));
        assert!(renderer.needs_render());
    }

    #[test]
    fn test_software_mode() {
        let mut renderer = MetaCursorRenderer::new();
        renderer.set_cursor(Some(CursorSprite::new(1, 32, 32)));

        let mut backend = MockBackend {
            can_hw: false,
            set_called: false,
            unset_called: false,
        };
        renderer.update_cursor(Some(&mut backend));

        assert_eq!(renderer.mode(), CursorRenderMode::Software);
        assert!(backend.unset_called);
    }

    #[test]
    fn test_hardware_mode() {
        let mut renderer = MetaCursorRenderer::new();
        renderer.set_cursor(Some(CursorSprite::new(1, 32, 32)));

        let mut backend = MockBackend {
            can_hw: true,
            set_called: false,
            unset_called: false,
        };
        renderer.update_cursor(Some(&mut backend));

        assert_eq!(renderer.mode(), CursorRenderMode::Hardware);
        assert!(backend.set_called);
    }

    #[test]
    fn test_disabled_cursor() {
        let mut renderer = MetaCursorRenderer::new();
        renderer.set_cursor(Some(CursorSprite::new(1, 32, 32)));
        renderer.set_enabled(false);

        let mut backend = MockBackend {
            can_hw: true,
            set_called: false,
            unset_called: false,
        };
        renderer.update_cursor(Some(&mut backend));

        assert_eq!(renderer.mode(), CursorRenderMode::Hidden);
        assert!(backend.unset_called);
    }

    #[test]
    fn test_cursor_rect() {
        let mut renderer = MetaCursorRenderer::new();
        let mut sprite = CursorSprite::new(1, 32, 32);
        sprite.set_hotspot(8, 8);
        renderer.set_cursor(Some(sprite));
        renderer.set_position(100, 100);

        let rect = renderer.cursor_rect().unwrap();
        assert_eq!(rect, CursorRect::new(92, 92, 32, 32));
    }

    #[test]
    fn test_no_sprite_no_rect() {
        let renderer = MetaCursorRenderer::new();
        assert!(renderer.cursor_rect().is_none());
    }
}
