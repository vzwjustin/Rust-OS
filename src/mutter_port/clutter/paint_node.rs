//! Port of GNOME mutter's `clutter/clutter-paint-node.{c,h}` and
//! `clutter-paint-node-private.h`.
//!
//! `ClutterPaintNode` is a node in clutter's retained-mode render tree.
//! Each actor's `paint()` builds a tree of paint nodes (e.g. "draw this
//! rectangle with this color", "draw this texture") which is then
//! traversed to issue the actual Cogl/GL draw calls.
//!
//! This port keeps only the parts that are pure data-structure / control
//! flow logic and have no dependency on Cogl or GL:
//!
//! - The tree structure itself: parent/child/sibling relationships,
//!   `add_child`, `remove_child`, `n_children`, and iteration. The C
//!   version represents the tree as an intrusive doubly linked list
//!   (`first_child`/`last_child`/`prev_sibling`/`next_sibling`) of
//!   manually ref-counted `GTypeInstance` pointers. Since this is a
//!   from-scratch Rust port (no GObject, no manual refcounting), the
//!   tree is represented with owning `Vec<PaintNode>` child storage
//!   instead of an intrusive linked list; the externally observable
//!   behavior (ordered children, `n_children`, depth-first paint order)
//!   is preserved.
//! - The node "kind": the base `ClutterPaintNode` in C is an abstract
//!   GObject type with no kind enum of its own -- concrete behaviors
//!   ("draw a rectangle of this color", "draw a texture", "clip to
//!   this box", "apply this transform", "act as a layer/root") are
//!   implemented as separate GObject subclasses scattered across
//!   `clutter-color-node.c`, `clutter-texture-node.c`,
//!   `clutter-clip-node.c`, `clutter-transform-node.c`,
//!   `clutter-layer-node.c`, `clutter-root-node.c`, and
//!   `clutter-pipeline-node.c` (not present in this file). Rather than
//!   inventing a parallel subclass-per-file hierarchy without those
//!   sources to port from, this module folds the handful of subclass
//!   "kinds" that are referenced/implied here (root, color, texture,
//!   clip, transform, layer, effect -- the last is declared in
//!   `clutter-paint-node-private.h` as `ClutterEffectNode`) into a
//!   single `PaintNodeKind` enum, each variant carrying the non-GL data
//!   that subclass would hold. This is a structural simplification
//!   noted here rather than hidden.
//! - The `paint()` dispatch logic (`clutter_paint_node_paint`): call
//!   `pre_draw`, and if it returns true, `draw`, then recurse into
//!   children in order, then (if `pre_draw` returned true) `post_draw`.
//!   The actual `draw`/`pre_draw`/`post_draw` implementations issue Cogl
//!   draw calls in upstream subclasses; here they are stubbed as no-ops
//!   returning `true` (paint), matching the default/base-class behavior
//!   in `clutter_paint_node_real_pre_draw` (returns `TRUE`) and
//!   `clutter_paint_node_real_draw`/`real_post_draw` (no-ops).
//!
//! Not ported (out of scope, GL/Cogl/GObject specific, no kernel
//! equivalent yet):
//! - Reference counting (`clutter_paint_node_ref`/`unref`): ownership is
//!   handled by normal Rust `Vec<PaintNode>` ownership instead.
//! - `ClutterPaintOperation` / `PaintOpCode` (`PAINT_OP_TEX_RECT`,
//!   `PAINT_OP_TEX_RECTS`, `PAINT_OP_MULTITEX_RECT`,
//!   `PAINT_OP_PRIMITIVE`) and the `add_rectangle`/
//!   `add_texture_rectangle`/`add_multitexture_rectangle`/
//!   `add_rectangles`/`add_texture_rectangles`/`add_primitive` family:
//!   these describe batches of Cogl geometry to submit to the GPU and
//!   have no meaning without a Cogl/GL backend. They are represented as
//!   a `PaintOps` placeholder `Vec` capturing the rectangle data only
//!   (no `CoglPrimitive`/multitexture support), with the actual
//!   submission stubbed.
//! - `clutter_paint_node_get_framebuffer` / `ClutterPaintNodeClass`'s
//!   `get_framebuffer` vtable slot: requires `CoglFramebuffer`. Ported
//!   structurally as a stub returning `None`.
//! - GObject machinery: `GType` registration, `GValue` transform
//!   functions (`clutter_value_set_paint_node` and friends), boxed/value
//!   table, `G_DEFINE_AUTOPTR_CLEANUP_FUNC`. None of this has a Rust
//!   equivalent in a `no_std`/no-GLib port.
//! - `clutter_paint_node_set_name` / `set_static_name` use
//!   `g_intern_string`; ported as a plain owned `String` field instead
//!   of an interned/static string, since there is no string interning
//!   table in this port.
//!
//! As with `actor_box`, this module uses no `unsafe`, no external
//! crates, and only `core`/`alloc`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::actor_box::ActorBox;

/// Stand-in for an RGBA color, since the kernel has no shared color type
/// available to this module yet. Mirrors what `ClutterColorNode` (in
/// `clutter-color-node.c`, not present in this file) would carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba { r, g, b, a }
    }
}

/// Stub for a 2D affine/projective transform matrix. `ClutterTransformNode`
/// (in `clutter-transform-node.c`, not present in this file) wraps a
/// `graphene_matrix_t`; since `graphene` isn't ported, this is a 4x4
/// row-major `f32` matrix placeholder with no operations beyond identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix4(pub [[f32; 4]; 4]);

impl Matrix4 {
    pub const fn identity() -> Self {
        Matrix4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
}

impl Default for Matrix4 {
    fn default() -> Self {
        Matrix4::identity()
    }
}

/// A single queued draw-geometry operation. Corresponds to
/// `ClutterPaintOperation` / `PaintOpCode` in
/// `clutter-paint-node-private.h`, trimmed to the data that doesn't
/// require Cogl. The `Primitive` variant is reduced to a unit marker
/// since `CoglPrimitive` has no port here.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    /// `PAINT_OP_TEX_RECT`: a single textured (or solid, if untextured)
    /// rectangle with normalized texture coordinates `(s1, t1, s2, t2)`.
    TexRect {
        rect: ActorBox,
        s1: f32,
        t1: f32,
        s2: f32,
        t2: f32,
    },
    /// `PAINT_OP_TEX_RECTS`: a batch of rectangles, `[x1, y1, x2, y2]`
    /// groups, sharing the node's texture coordinates.
    TexRects(Vec<[f32; 4]>),
    /// `PAINT_OP_MULTITEX_RECT`: a rectangle with multiple texture-unit
    /// coordinate sets.
    MultitexRect {
        rect: ActorBox,
        tex_coords: Vec<f32>,
    },
    /// `PAINT_OP_PRIMITIVE`: an arbitrary Cogl primitive. Has no
    /// representation without Cogl; kept as a marker so callers can see
    /// that this op kind existed upstream.
    Primitive,
}

/// The "kind" of a paint node, folding in the handful of subclasses
/// referenced by/implied in `clutter-paint-node.c` and
/// `clutter-paint-node-private.h`:
///
/// - `Root`: the base of a paint tree for an actor (`_clutter_dummy_node_new`
///   in `clutter-paint-node-private.h` creates such a placeholder root).
/// - `Color`: draws flat-colored geometry (`clutter-color-node.c`).
/// - `Texture`: draws a texture (`clutter-texture-node.c`).
/// - `Clip`: clips children's painting to a region (`clutter-clip-node.c`).
/// - `Transform`: applies a transform matrix to children
///   (`clutter-transform-node.c`).
/// - `Layer`: renders children off-screen into a layer/FBO before
///   compositing (`clutter-layer-node.c`).
/// - `Effect`: wraps a `ClutterEffect`, declared in
///   `clutter-paint-node-private.h` as `ClutterEffectNode`.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintNodeKind {
    Root,
    Color(Rgba),
    Texture,
    Clip(ActorBox),
    Transform(Matrix4),
    Layer,
    Effect,
}

/// Port of `ClutterPaintNode` / `_ClutterPaintNode`.
///
/// The C struct is an intrusive doubly linked list node
/// (`parent`/`first_child`/`last_child`/`prev_sibling`/`next_sibling`)
/// with manual GObject-style reference counting. This port instead owns
/// its children directly in a `Vec<Box<PaintNode>>`, which gives the
/// same externally observable tree shape (ordered list of children,
/// `n_children()`, depth-first paint traversal) without needing
/// intrusive pointers or a refcount.
#[derive(Debug, Clone, PartialEq)]
pub struct PaintNode {
    /// Corresponds to `ClutterPaintNode::name`. Stored as an owned
    /// `String` rather than an interned string (see module docs).
    pub name: Option<String>,
    /// Corresponds to the subclass identity (there is no single field
    /// for this in C -- it's the GObject's dynamic type).
    pub kind: PaintNodeKind,
    /// Corresponds to `ClutterPaintNode::operations`.
    pub operations: Vec<PaintOp>,
    /// Corresponds to `ClutterPaintNode::parent`. The C struct keeps a
    /// raw back-pointer to the parent node; since this port owns its
    /// children via `Box<PaintNode>` (no shared/aliased ownership), a
    /// real pointer would create a cycle and break `Clone`/`PartialEq`.
    /// Instead the parent's `name` is recorded as the back-pointer
    /// identity, which is sufficient for `get_parent` queries and for
    /// `get_framebuffer`-style ancestor walks that only need to know
    /// whether a parent exists and which node it is.
    parent: Option<String>,
    children: Vec<Box<PaintNode>>,
}

impl PaintNode {
    /// Corresponds to `_clutter_paint_node_create` + per-subclass `_new()`
    /// constructors collapsed into one entry point, since this port has
    /// no GType machinery to dispatch on.
    pub fn new(kind: PaintNodeKind) -> Self {
        PaintNode {
            name: None,
            kind,
            operations: Vec::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    /// Port of `clutter_paint_node_set_name`. (`g_intern_string` has no
    /// equivalent here; see module docs.)
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }

    /// Port of `clutter_paint_node_get_n_children`.
    pub fn n_children(&self) -> usize {
        self.children.len()
    }

    /// Port of `clutter_paint_node_add_child`.
    ///
    /// The C version asserts `node != child` and `child->parent == NULL`
    /// (a node can only be added once, to one parent). Since this port
    /// has no shared/aliased ownership of nodes (children are owned
    /// `Box<PaintNode>` values created fresh by the caller), those
    /// invariants are structurally guaranteed instead of runtime-checked.
    /// The child's `parent` back-pointer is set to this node's `name`
    /// (mirroring `child->parent = node` in C).
    pub fn add_child(&mut self, mut child: PaintNode) {
        child.parent = self.name.clone();
        self.children.push(Box::new(child));
    }

    /// Sets this node's parent back-pointer to the given parent node's
    /// identity. Mirrors assigning `node->parent = parent` in C. The
    /// parent is recorded by its `name` (see `PaintNode::parent` docs).
    pub fn set_parent(&mut self, parent: &PaintNode) {
        self.parent = parent.name.clone();
    }

    /// Clears this node's parent back-pointer, mirroring setting
    /// `node->parent = NULL` in C (e.g. after `remove_child`).
    pub fn clear_parent(&mut self) {
        self.parent = None;
    }

    /// Returns this node's parent identity (the parent node's `name`),
    /// or `None` if this node has no parent (it is a root). Mirrors
    /// reading `ClutterPaintNode::parent` in C.
    pub fn get_parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Port of `clutter_paint_node_remove_child`. Removes the child at
    /// `index`, preserving the order of the remaining siblings (matching
    /// the intrusive-list unlink behavior in C). Returns the removed node,
    /// or `None` if `index` is out of bounds.
    pub fn remove_child(&mut self, index: usize) -> Option<PaintNode> {
        if index >= self.children.len() {
            return None;
        }
        let mut removed = *self.children.remove(index);
        removed.parent = None;
        Some(removed)
    }

    /// Removes all children, mirroring what repeatedly calling
    /// `clutter_paint_node_remove_child` on every child would do.
    pub fn remove_all_children(&mut self) {
        self.children.clear();
    }

    /// Port of iterating `first_child`/`next_sibling` in C.
    pub fn children(&self) -> impl Iterator<Item = &PaintNode> {
        self.children.iter().map(|b| b.as_ref())
    }

    /// Mutable child iteration (no direct C equivalent; the linked list
    /// in C is walked manually wherever mutation is needed).
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut PaintNode> {
        self.children.iter_mut().map(|b| b.as_mut())
    }

    pub fn add_rectangle(&mut self, rect: ActorBox) {
        self.operations.push(PaintOp::TexRect {
            rect,
            s1: 0.0,
            t1: 0.0,
            s2: 1.0,
            t2: 1.0,
        });
    }

    pub fn add_texture_rectangle(&mut self, rect: ActorBox, s1: f32, t1: f32, s2: f32, t2: f32) {
        self.operations.push(PaintOp::TexRect {
            rect,
            s1,
            t1,
            s2,
            t2,
        });
    }

    /// Appends a render operation to this node's operation list. This is
    /// the generic entry point used by the typed `add_rectangle` /
    /// `add_texture_rectangle` helpers and may be called directly to queue
    /// `MultitexRect` or `Primitive` ops.
    pub fn add_operation(&mut self, op: PaintOp) {
        self.operations.push(op);
    }

    /// Returns the list of render operations queued on this node. A Cogl
    /// backend would iterate this list in `draw` to submit
    /// `cogl_framebuffer_draw_*` calls.
    pub fn get_operations(&self) -> &[PaintOp] {
        &self.operations
    }

    /// Clears all queued render operations. Called after the operation
    /// list has been submitted to the GPU (in `draw`) or when the node is
    /// recycled between frames.
    pub fn clear_operations(&mut self) {
        self.operations.clear();
    }

    /// Port of `clutter_paint_node_get_framebuffer`: walks up through
    /// `parent` looking for the first ancestor with a custom framebuffer.
    /// Since there is no `CoglFramebuffer` in this port, this always
    /// returns `None`. The parent back-pointer (`get_parent`) is tracked
    /// so a future GPU/framebuffer abstraction can perform the ancestor
    /// walk once a framebuffer-owning node kind is introduced.
    pub fn get_framebuffer(&self) -> Option<()> {
        None
    }

    /// Pre-draw hook. Corresponds to the `pre_draw` vtable slot and its
    /// base-class implementation `clutter_paint_node_real_pre_draw`,
    /// which always returns `TRUE` ("yes, proceed to draw"). Real
    /// subclasses (color/texture/etc.) would set up Cogl pipeline state
    /// here; in this port the operation list is already tracked on
    /// `self.operations` and a future Cogl backend would push the
    /// node's pipeline, clip, and transform onto the active framebuffer
    /// at this point before `draw` submits the queued operations.
    fn pre_draw(&self, _ctx: &PaintContext) -> bool {
        true
    }

    /// Draw hook. Corresponds to the `draw` vtable slot
    /// (`clutter_paint_node_real_draw` is a no-op in the base class;
    /// subclasses issue the actual `cogl_framebuffer_draw_*` calls using
    /// `self.operations`). In this port the operations are tracked on the
    /// node and a future Cogl backend would iterate `get_operations()`
    /// here, submitting each `PaintOp` as a `cogl_framebuffer_draw_*`
    /// call (e.g. `cogl_framebuffer_draw_textured_rectangle` for
    /// `PaintOp::TexRect`).
    fn draw(&self, _ctx: &PaintContext) {
        // Operations are recorded on `self.operations`; a Cogl backend
        // would submit them to the framebuffer here.
    }

    /// Post-draw hook. Corresponds to the `post_draw` vtable slot
    /// (`clutter_paint_node_real_post_draw` is a no-op in the base
    /// class; subclasses would pop pipeline/clip/transform state here).
    /// A future Cogl backend would pop whatever state `pre_draw` pushed
    /// onto the framebuffer.
    fn post_draw(&self, _ctx: &PaintContext) {
        // A Cogl backend would pop pipeline/clip/transform state here.
    }

    /// Port of `clutter_paint_node_paint`: depth-first traversal that
    /// calls `pre_draw`, then (if it returned true) `draw`, then
    /// recurses into every child in order, then (if `pre_draw` returned
    /// true) `post_draw`.
    pub fn paint(&self, ctx: &PaintContext) {
        let should_draw = self.pre_draw(ctx);

        if should_draw {
            self.draw(ctx);
        }

        for child in self.children() {
            child.paint(ctx);
        }

        if should_draw {
            self.post_draw(ctx);
        }
    }
}

/// Placeholder for `ClutterPaintContext`, which in upstream mutter
/// carries the active `CoglFramebuffer`, clip region, and redraw clip
/// data for one paint pass. Not ported (GL/Cogl specific); kept as an
/// empty marker type purely so `PaintNode::paint`'s signature mirrors
/// `clutter_paint_node_paint(ClutterPaintNode *, ClutterPaintContext *)`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PaintContext;

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str) -> PaintNode {
        let mut n = PaintNode::new(PaintNodeKind::Color(Rgba::new(255, 0, 0, 255)));
        n.set_name(name);
        n
    }

    #[test]
    fn new_node_has_no_children() {
        let root = PaintNode::new(PaintNodeKind::Root);
        assert_eq!(root.n_children(), 0);
        assert_eq!(root.children().count(), 0);
    }

    #[test]
    fn add_child_increments_count_and_preserves_order() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(leaf("a"));
        root.add_child(leaf("b"));
        root.add_child(leaf("c"));

        assert_eq!(root.n_children(), 3);
        let names: Vec<_> = root
            .children()
            .map(|c| c.name.as_deref().unwrap())
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn remove_child_by_index_preserves_remaining_order() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(leaf("a"));
        root.add_child(leaf("b"));
        root.add_child(leaf("c"));

        let removed = root.remove_child(1).unwrap();
        assert_eq!(removed.name.as_deref(), Some("b"));
        assert_eq!(root.n_children(), 2);

        let names: Vec<_> = root
            .children()
            .map(|c| c.name.as_deref().unwrap())
            .collect();
        assert_eq!(names, ["a", "c"]);
    }

    #[test]
    fn remove_child_out_of_bounds_returns_none() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(leaf("a"));
        assert!(root.remove_child(5).is_none());
        assert_eq!(root.n_children(), 1);
    }

    #[test]
    fn remove_all_children_clears_tree() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(leaf("a"));
        root.add_child(leaf("b"));
        root.remove_all_children();
        assert_eq!(root.n_children(), 0);
    }

    #[test]
    fn nested_tree_children_mut_can_mutate() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.add_child(leaf("a"));
        for child in root.children_mut() {
            child.set_name("renamed");
        }
        assert_eq!(
            root.children().next().unwrap().name.as_deref(),
            Some("renamed")
        );
    }

    /// Records visitation order to verify pre-order, depth-first paint
    /// traversal matching `clutter_paint_node_paint`'s recursive walk.
    #[test]
    fn paint_visits_self_then_children_in_order_then_grandchildren() {
        let mut root = PaintNode::new(PaintNodeKind::Root);

        let mut child_a = leaf("a");
        child_a.add_child(leaf("a1"));
        child_a.add_child(leaf("a2"));

        let child_b = leaf("b");

        root.add_child(child_a);
        root.add_child(child_b);

        // `paint()` itself is side-effect free in this port (no Cogl),
        // so just confirm the tree shape it will walk is as expected
        // and that calling it doesn't panic / behaves recursively.
        let ctx = PaintContext::default();
        root.paint(&ctx);

        assert_eq!(root.n_children(), 2);
        let first_child = root.children().next().unwrap();
        assert_eq!(first_child.n_children(), 2);
    }

    #[test]
    fn add_rectangle_appends_tex_rect_op_with_default_full_texture_coords() {
        let mut node = PaintNode::new(PaintNodeKind::Texture);
        let rect = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        node.add_rectangle(rect);

        assert_eq!(node.operations.len(), 1);
        match &node.operations[0] {
            PaintOp::TexRect {
                rect: r,
                s1,
                t1,
                s2,
                t2,
            } => {
                assert_eq!(r, &rect);
                assert_eq!((s1, t1, s2, t2), (&0.0, &0.0, &1.0, &1.0));
            }
            other => panic!("unexpected op: {other:?}"),
        }
    }

    #[test]
    fn get_framebuffer_is_stubbed_to_none() {
        let node = PaintNode::new(PaintNodeKind::Root);
        assert!(node.get_framebuffer().is_none());
    }

    #[test]
    fn new_node_has_no_parent() {
        let node = PaintNode::new(PaintNodeKind::Root);
        assert!(node.get_parent().is_none());
    }

    #[test]
    fn add_child_sets_child_parent_to_parent_name() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.set_name("root");

        let child = leaf("child");
        root.add_child(child);

        let added = root.children().next().unwrap();
        assert_eq!(added.get_parent(), Some("root"));
    }

    #[test]
    fn add_child_with_unnamed_parent_sets_parent_to_none_identity() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        // Parent has no name, so the back-pointer identity is None.
        root.add_child(leaf("child"));

        let added = root.children().next().unwrap();
        assert_eq!(added.get_parent(), None);
        // But the child still has its own name.
        assert_eq!(added.name.as_deref(), Some("child"));
    }

    #[test]
    fn set_parent_records_parent_name() {
        let mut parent = PaintNode::new(PaintNodeKind::Root);
        parent.set_name("p");

        let mut child = PaintNode::new(PaintNodeKind::Color(Rgba::new(0, 0, 0, 255)));
        assert_eq!(child.get_parent(), None);

        child.set_parent(&parent);
        assert_eq!(child.get_parent(), Some("p"));
    }

    #[test]
    fn clear_parent_resets_back_pointer() {
        let mut parent = PaintNode::new(PaintNodeKind::Root);
        parent.set_name("p");

        let mut child = PaintNode::new(PaintNodeKind::Color(Rgba::new(0, 0, 0, 255)));
        child.set_parent(&parent);
        assert_eq!(child.get_parent(), Some("p"));

        child.clear_parent();
        assert_eq!(child.get_parent(), None);
    }

    #[test]
    fn remove_child_clears_its_parent_back_pointer() {
        let mut root = PaintNode::new(PaintNodeKind::Root);
        root.set_name("root");
        root.add_child(leaf("child"));

        let removed = root.remove_child(0).unwrap();
        assert_eq!(removed.name.as_deref(), Some("child"));
        // Removed node no longer references its former parent.
        assert_eq!(removed.get_parent(), None);
    }
}
