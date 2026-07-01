//! Port of GNOME mutter's `clutter/clutter-paint-nodes.{c,h}`.
//!
//! Concrete paint node subclasses forming the render tree.
//!
//! # What's ported
//!
//! - `ColorNode`, `TextureNode`, `PipelineNode`, `ClipNode`,
//!   `TransformNode`, `RootNode`, `LayerNode`, `ActorNode`.
//! - Tree-building helpers: `build_tree`, `traverse_pre_order`, `count_nodes`.
//! - `PaintFlags` bitfield.
//!
//! # What's skipped
//!
//! - Cogl pipeline/texture/framebuffer creation: opaque `u32` handles.
//! - GObject subclassing machinery: replaced by typed Rust structs.

#![allow(dead_code)]

use alloc::vec::Vec;

use super::super::actor::ActorId;
use super::super::actor_box::ActorBox;
use super::super::paint_node::{Matrix4, PaintNode, PaintNodeKind, PaintOp, PaintContext, Rgba};
use super::backend::ScalingFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FramebufferHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OffscreenHandle(pub u32);

/// Paint flags, mirroring `ClutterPaintFlag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PaintFlags(pub u32);

impl PaintFlags {
    pub const NONE: Self = Self(0);
    pub const NO_CURSORS: Self = Self(1 << 0);
    pub const FORCE_CURSORS: Self = Self(1 << 1);
    pub const CLEAR: Self = Self(1 << 2);
    pub fn union(self, other: Self) -> Self { PaintFlags(self.0 | other.0) }
    pub fn contains(self, flag: Self) -> bool { (self.0 & flag.0) == flag.0 }
}

/// Port of `ClutterColorNode`. Draws solid-colored geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorNode {
    pub node: PaintNode,
    color: Rgba,
}

impl ColorNode {
    pub fn new(color: Rgba) -> Self {
        ColorNode { node: PaintNode::new(PaintNodeKind::Color(color)), color }
    }
    pub fn color(&self) -> Rgba { self.color }
    pub fn set_color(&mut self, color: Rgba) {
        self.color = color;
        self.node.kind = PaintNodeKind::Color(color);
    }
    pub fn add_rectangle(&mut self, rect: ActorBox) { self.node.add_rectangle(rect); }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterTextureNode`. Draws a texture with tint and filters.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureNode {
    pub node: PaintNode,
    texture: TextureHandle,
    tint_color: Rgba,
    min_filter: ScalingFilter,
    mag_filter: ScalingFilter,
}

impl TextureNode {
    pub fn new(texture: TextureHandle, tint_color: Rgba, min_filter: ScalingFilter, mag_filter: ScalingFilter) -> Self {
        TextureNode { node: PaintNode::new(PaintNodeKind::Texture), texture, tint_color, min_filter, mag_filter }
    }
    pub fn texture(&self) -> TextureHandle { self.texture }
    pub fn tint_color(&self) -> Rgba { self.tint_color }
    pub fn min_filter(&self) -> ScalingFilter { self.min_filter }
    pub fn mag_filter(&self) -> ScalingFilter { self.mag_filter }
    pub fn add_texture_rectangle(&mut self, rect: ActorBox, s1: f32, t1: f32, s2: f32, t2: f32) {
        self.node.add_texture_rectangle(rect, s1, t1, s2, t2);
    }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterPipelineNode`. Wraps a raw CoglPipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineNode {
    pub node: PaintNode,
    pipeline: PipelineHandle,
}

impl PipelineNode {
    pub fn new(pipeline: PipelineHandle) -> Self {
        PipelineNode { node: PaintNode::new(PaintNodeKind::Texture), pipeline }
    }
    pub fn pipeline(&self) -> PipelineHandle { self.pipeline }
    pub fn set_pipeline(&mut self, pipeline: PipelineHandle) { self.pipeline = pipeline; }
    pub fn add_rectangle(&mut self, rect: ActorBox) { self.node.add_rectangle(rect); }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterClipNode`. Clips children to a rectangular region.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNode {
    pub node: PaintNode,
    clip: ActorBox,
}

impl ClipNode {
    pub fn new(clip: ActorBox) -> Self {
        ClipNode { node: PaintNode::new(PaintNodeKind::Clip(clip)), clip }
    }
    pub fn clip(&self) -> ActorBox { self.clip }
    pub fn set_clip(&mut self, clip: ActorBox) {
        self.clip = clip;
        self.node.kind = PaintNodeKind::Clip(clip);
    }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterTransformNode`. Applies a modelview transform.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformNode {
    pub node: PaintNode,
    matrix: Matrix4,
}

impl TransformNode {
    pub fn new(matrix: Matrix4) -> Self {
        TransformNode { node: PaintNode::new(PaintNodeKind::Transform(matrix)), matrix }
    }
    pub fn matrix(&self) -> Matrix4 { self.matrix }
    pub fn set_matrix(&mut self, matrix: Matrix4) {
        self.matrix = matrix;
        self.node.kind = PaintNodeKind::Transform(matrix);
    }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterRootNode`. Root of a paint tree for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct RootNode {
    pub node: PaintNode,
    framebuffer: FramebufferHandle,
    clip_region: Option<u32>,
    paint_flags: PaintFlags,
}

impl RootNode {
    pub fn new(framebuffer: FramebufferHandle, clip_region: Option<u32>, paint_flags: PaintFlags) -> Self {
        RootNode { node: PaintNode::new(PaintNodeKind::Root), framebuffer, clip_region, paint_flags }
    }
    pub fn framebuffer(&self) -> FramebufferHandle { self.framebuffer }
    pub fn clip_region(&self) -> Option<u32> { self.clip_region }
    pub fn paint_flags(&self) -> PaintFlags { self.paint_flags }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterLayerNode`. Renders children off-screen then composites.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerNode {
    pub node: PaintNode,
    offscreen: OffscreenHandle,
    pipeline: PipelineHandle,
}

impl LayerNode {
    pub fn new(offscreen: OffscreenHandle, pipeline: PipelineHandle) -> Self {
        LayerNode { node: PaintNode::new(PaintNodeKind::Layer), offscreen, pipeline }
    }
    pub fn offscreen(&self) -> OffscreenHandle { self.offscreen }
    pub fn pipeline(&self) -> PipelineHandle { self.pipeline }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Port of `ClutterActorNode`. Paints a single actor with opacity override.
#[derive(Debug, Clone, PartialEq)]
pub struct ActorNode {
    pub node: PaintNode,
    actor: ActorId,
    opacity: u8,
}

impl ActorNode {
    pub fn new(actor: ActorId, opacity: u8) -> Self {
        ActorNode { node: PaintNode::new(PaintNodeKind::Root), actor, opacity }
    }
    pub fn actor(&self) -> ActorId { self.actor }
    pub fn opacity(&self) -> u8 { self.opacity }
    pub fn set_opacity(&mut self, opacity: u8) { self.opacity = opacity; }
    pub fn into_node(self) -> PaintNode { self.node }
}

/// Builds a `PaintNode` tree from a root and children.
pub fn build_tree(root: PaintNode, children: Vec<PaintNode>) -> PaintNode {
    let mut root = root;
    for child in children { root.add_child(child); }
    root
}

/// Traverses a paint tree depth-first pre-order.
pub fn traverse_pre_order<F: FnMut(&PaintNode)>(node: &PaintNode, f: &mut F) {
    f(node);
    for child in node.children() { traverse_pre_order(child, f); }
}

/// Counts total nodes in a paint tree.
pub fn count_nodes(node: &PaintNode) -> usize {
    let mut count = 1;
    for child in node.children() { count += count_nodes(child); }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Rgba { Rgba::new(255, 0, 0, 255) }
    fn blue() -> Rgba { Rgba::new(0, 0, 255, 255) }
    fn box01() -> ActorBox { ActorBox::new(0.0, 0.0, 10.0, 10.0) }

    #[test]
    fn color_node_creation() {
        let node = ColorNode::new(red());
        assert_eq!(node.color(), red());
        assert!(matches!(node.node.kind, PaintNodeKind::Color(_)));
    }

    #[test]
    fn color_node_set_color() {
        let mut node = ColorNode::new(red());
        node.set_color(blue());
        assert_eq!(node.color(), blue());
    }

    #[test]
    fn color_node_add_rectangle() {
        let mut node = ColorNode::new(red());
        node.add_rectangle(box01());
        assert_eq!(node.node.operations.len(), 1);
    }

    #[test]
    fn texture_node_creation() {
        let node = TextureNode::new(TextureHandle(1), red(), ScalingFilter::Linear, ScalingFilter::Nearest);
        assert_eq!(node.texture(), TextureHandle(1));
        assert_eq!(node.min_filter(), ScalingFilter::Linear);
    }

    #[test]
    fn texture_node_add_texture_rectangle() {
        let mut node = TextureNode::new(TextureHandle(1), red(), ScalingFilter::Linear, ScalingFilter::Linear);
        node.add_texture_rectangle(box01(), 0.0, 0.0, 1.0, 1.0);
        assert_eq!(node.node.operations.len(), 1);
    }

    #[test]
    fn pipeline_node_creation() {
        let node = PipelineNode::new(PipelineHandle(5));
        assert_eq!(node.pipeline(), PipelineHandle(5));
    }

    #[test]
    fn clip_node_creation() {
        let clip = box01();
        let node = ClipNode::new(clip);
        assert_eq!(node.clip(), clip);
    }

    #[test]
    fn transform_node_creation() {
        let matrix = Matrix4::identity();
        let node = TransformNode::new(matrix);
        assert_eq!(node.matrix(), matrix);
    }

    #[test]
    fn root_node_creation() {
        let node = RootNode::new(FramebufferHandle(1), Some(42), PaintFlags::NO_CURSORS);
        assert_eq!(node.framebuffer(), FramebufferHandle(1));
        assert_eq!(node.clip_region(), Some(42));
        assert!(node.paint_flags().contains(PaintFlags::NO_CURSORS));
    }

    #[test]
    fn layer_node_creation() {
        let node = LayerNode::new(OffscreenHandle(1), PipelineHandle(2));
        assert_eq!(node.offscreen(), OffscreenHandle(1));
        assert_eq!(node.pipeline(), PipelineHandle(2));
    }

    #[test]
    fn actor_node_creation() {
        let actor = ActorId { index: 0, generation: 1 };
        let node = ActorNode::new(actor, 128);
        assert_eq!(node.actor(), actor);
        assert_eq!(node.opacity(), 128);
    }

    #[test]
    fn build_tree_adds_children() {
        let root = PaintNode::new(PaintNodeKind::Root);
        let children = vec![ColorNode::new(red()).into_node(), ColorNode::new(blue()).into_node()];
        let tree = build_tree(root, children);
        assert_eq!(tree.n_children(), 2);
    }

    #[test]
    fn traverse_pre_order_visits_all() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        let mut a = ColorNode::new(red()).into_node();
        a.add_child(ColorNode::new(blue()).into_node());
        root.add_child(a);
        root.add_child(ColorNode::new(blue()).into_node());
        let mut visited = 0;
        traverse_pre_order(&root, &mut |_| visited += 1);
        assert_eq!(visited, 4);
    }

    #[test]
    fn count_nodes_counts_all_descendants() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(PaintNode::new(PaintNodeKind::Root));
        root.add_child(PaintNode::new(PaintNodeKind::Root));
        assert_eq!(count_nodes(&root), 3);
    }

    #[test]
    fn paint_flags_bitfield() {
        let flags = PaintFlags::NO_CURSORS.union(PaintFlags::CLEAR);
        assert!(flags.contains(PaintFlags::NO_CURSORS));
        assert!(flags.contains(PaintFlags::CLEAR));
        assert!(!flags.contains(PaintFlags::FORCE_CURSORS));
    }
}
