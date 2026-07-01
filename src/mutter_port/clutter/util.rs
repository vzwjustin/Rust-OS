//! Port of GNOME mutter's `clutter/clutter-util.{c,h}`.
//!
//! Vertex transformation utilities and coordinate scaling functions.
//! Port focuses on the core math: `Vertex4` struct and OpenGL-to-window
//! coordinate transformations. The C source also includes a GLib-based
//! progress function registry (for `ClutterInterval` animation interpolation),
//! which is omitted here as it is GObject type-system scaffolding without
//! equivalent in Rust; callers can use trait objects or closure-based
//! registries as needed.
//!
//! # What's ported
//!
//! - `ClutterVertex4` (a homogeneous coordinate tuple): `Vertex4` struct
//! - Three scaling macros converted to inline functions:
//!   - `mtx_gl_scale_x` — OpenGL [-1,1] X → window [0, viewport_width]
//!   - `mtx_gl_scale_y` — OpenGL [-1,1] Y → window [0, viewport_height]
//!     (Y is inverted per OpenGL convention)
//!   - `mtx_gl_scale_z` — same as X, for Z coordinate
//! - `_clutter_util_fully_transform_vertices()` function signature and
//!   coordinate scaling loop (vertices are transformed via matrix ops
//!   which are currently unimplemented as they require a graphene port).
//!
//! # What's skipped, with rationale
//!
//! - Progress function registry (`_clutter_has_progress_function`,
//!   `_clutter_run_progress_function`, `clutter_interval_register_progress_func`):
//!   The C implementation uses `GHashTable`, `GType`, `GValue`, and mutexes
//!   for a global registry. This is GLib/GObject scaffolding with no direct
//!   Rust equivalent; callers should use trait objects or lazy_static for
//!   similar functionality if needed.
//! - Matrix multiplication (`graphene_matrix_multiply` /
//!   `cogl_graphene_matrix_project_points_f3` / `cogl_graphene_matrix_transform_points`):
//!   These depend on graphene and COGL libraries, not yet ported to Rust in
//!   this codebase. The function signature is provided; implementation is left
//!   as a compile-time error (unimplemented) to allow the type to exist.
//! - `clutter_round_to_256ths`: Used in the coordinate scaling loop; can be
//!   replaced with `(value * 256.0).round() / 256.0` inline, or imported from
//!   a math utilities module if one exists.

/// A 4-element vertex with homogeneous coordinates (x, y, z, w).
///
/// Mirrors `ClutterVertex4` from mutter's `clutter` library. Used for
/// intermediate calculations in perspective-projected vertex transforms.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vertex4 {
    /// Creates a new vertex with the given coordinates.
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vertex4 { x, y, z, w }
    }
}

/// Scales an OpenGL X coordinate (in [-1, 1] range, divided by homogeneous w)
/// to window coordinate space [v2, v2 + v1].
///
/// # Arguments
///
/// - `x`: homogeneous X coordinate (pre-divided by w)
/// - `w`: homogeneous W coordinate
/// - `v1`: viewport width
/// - `v2`: viewport X offset
///
/// # Formula
///
/// Converts from OpenGL [-1, 1] to [0, viewport_width], then offsets by viewport_x:
/// `(((x / w + 1.0) / 2.0) * v1) + v2`
#[inline]
pub fn mtx_gl_scale_x(x: f32, w: f32, v1: f32, v2: f32) -> f32 {
    (((x / w + 1.0) / 2.0) * v1) + v2
}

/// Scales an OpenGL Y coordinate to window coordinate space.
///
/// Y is inverted in OpenGL (bottom = -1, top = 1) relative to window
/// coordinates (top = 0, bottom = height).
///
/// # Arguments
///
/// - `y`: homogeneous Y coordinate (pre-divided by w)
/// - `w`: homogeneous W coordinate
/// - `v1`: viewport height
/// - `v2`: viewport Y offset
///
/// # Formula
///
/// Converts from OpenGL [-1, 1] to [0, viewport_height] with inversion,
/// then offsets by viewport_y: `v1 - (((y / w + 1.0) / 2.0) * v1) + v2`
#[inline]
pub fn mtx_gl_scale_y(y: f32, w: f32, v1: f32, v2: f32) -> f32 {
    v1 - (((y / w + 1.0) / 2.0) * v1) + v2
}

/// Scales an OpenGL Z coordinate to window coordinate space.
///
/// Z uses the same transformation as X (no inversion).
#[inline]
pub fn mtx_gl_scale_z(z: f32, w: f32, v1: f32, v2: f32) -> f32 {
    mtx_gl_scale_x(z, w, v1, v2)
}

/// Rounds a value to 1/256th precision (used for rasterization).
///
/// This is a helper used in the vertex transformation loop for quantization.
#[inline]
pub fn round_to_256ths(value: f32) -> f32 {
    libm::roundf(value * 256.0) / 256.0
}
