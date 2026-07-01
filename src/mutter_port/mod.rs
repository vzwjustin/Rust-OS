//! Staged port of GNOME mutter to RustOS. `mtk` and `clutter` are ports of
//! mutter's vendored support libraries; `core`, `compositor`, `backends`,
//! and `wayland` mirror mutter's own `src/core`, `src/compositor`,
//! `src/backends`, and `src/wayland` directories.

pub mod backends;
pub mod clutter;
pub mod compositor;
pub mod core;
pub mod math;
pub mod meta;
pub mod mtk;
pub mod wayland;
