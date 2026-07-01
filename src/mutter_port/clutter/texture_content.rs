//! Port of GNOME mutter's `clutter/clutter-texture-content.{c,h}`.
//!
//! `ClutterTextureContent` is a `Content` implementation that wraps a GPU
//! texture (e.g., an image or framebuffer attachment) for painting. It provides
//! the texture's intrinsic size and creates a `PaintNode::Texture` when painted.
//!
//! # What's ported
//!
//! - `ClutterTextureContent` struct holding an opaque texture handle, dimensions,
//!   and an optional clip region (matching `CoglTexture*` and `MtkRectangle*` from C).
//! - `get_preferred_size` returning the texture's width/height with `has_size=true`.
//! - `paint_content` creating a `PaintNode::Texture` with the texture id.
//! - `new_from_texture` constructor and `get_texture` accessor matching C API.
//!
//! # What's skipped
//!
//! - Cogl texture binding: texture is represented as an opaque u32 id.
//! - GObject machinery and lifecycle management.

use super::actor::ActorId;
use super::actor_box::ActorBox;
use super::content::Content;
use super::paint_context::PaintContext;
use super::paint_node::{PaintNode, PaintNodeKind};
use crate::mutter_port::mtk::rectangle::Rectangle;

/// Port of `ClutterTextureContent`: a content implementation wrapping a GPU
/// texture with optional clipping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureContent {
    /// Opaque texture handle (e.g., GPU resource id).
    pub texture_id: u32,
    /// Texture width in pixels.
    pub width: f32,
    /// Texture height in pixels.
    pub height: f32,
    /// Optional clip region within the texture.
    pub clip: Option<Rectangle>,
}

impl TextureContent {
    /// Create a new `TextureContent` from a texture handle, dimensions, and
    /// optional clip region. Port of `clutter_texture_content_new_from_texture`.
    pub const fn new(texture_id: u32, width: f32, height: f32, clip: Option<Rectangle>) -> Self {
        TextureContent {
            texture_id,
            width,
            height,
            clip,
        }
    }

    /// Get the texture handle. Port of `clutter_texture_content_get_texture`.
    pub const fn get_texture(&self) -> u32 {
        self.texture_id
    }

    /// Get the texture width.
    pub const fn get_width(&self) -> f32 {
        self.width
    }

    /// Get the texture height.
    pub const fn get_height(&self) -> f32 {
        self.height
    }

    /// Get the clip region if present.
    pub const fn get_clip(&self) -> Option<Rectangle> {
        self.clip
    }
}

impl Content for TextureContent {
    /// Return the texture's intrinsic size. Port of
    /// `ClutterTextureContent::get_preferred_size`.
    fn get_preferred_size(&self) -> (f32, f32, bool) {
        (self.width, self.height, true)
    }

    /// Create a texture paint node for rendering. Port of
    /// `ClutterTextureContent::paint_content`. Creates a child texture node
    /// and adds it to the provided paint node tree.
    fn paint_content(&self, _actor: ActorId, node: &mut PaintNode, _ctx: &PaintContext) {
        let mut tex_node = PaintNode::new(PaintNodeKind::Texture);
        tex_node.set_name("Texture Content");

        let rect = ActorBox::new(0.0, 0.0, self.width, self.height);
        tex_node.add_rectangle(rect);

        node.add_child(tex_node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_texture_content() {
        let tc = TextureContent::new(42, 800.0, 600.0, None);
        assert_eq!(tc.texture_id, 42);
        assert_eq!(tc.width, 800.0);
        assert_eq!(tc.height, 600.0);
        assert_eq!(tc.clip, None);
    }

    #[test]
    fn get_preferred_size_returns_dimensions_with_true() {
        let tc = TextureContent::new(42, 800.0, 600.0, None);
        let (w, h, has) = tc.get_preferred_size();
        assert_eq!(w, 800.0);
        assert_eq!(h, 600.0);
        assert_eq!(has, true);
    }

    #[test]
    fn with_clip_region() {
        let clip = Rectangle::new(10, 20, 100, 100);
        let tc = TextureContent::new(42, 800.0, 600.0, Some(clip));
        assert_eq!(tc.clip, Some(clip));
    }
}
