//! Port of GNOME mutter's `clutter/clutter-backend.{c,h}`,
//! `clutter/clutter-main.{c,h}`, `clutter/clutter-paint-nodes.{c,h}`,
//! `clutter/clutter-pipeline-cache.{c,h}`, and
//! `clutter/clutter-accessibility.{c,h}`.
//!
//! Each submodule ports one C source pair into a single `.rs` file.

pub mod accessibility;
pub mod backend;
pub mod main;
pub mod paint_nodes;
pub mod pipeline_cache;
