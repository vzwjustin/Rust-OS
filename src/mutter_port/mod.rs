//! Staged port of GNOME mutter to RustOS. `mtk` and `clutter` are ports of
//! mutter's vendored support libraries; `core`, `compositor`, `backends`,
//! `wayland`, `x11`, and `frames` mirror mutter's own `src/core`,
//! `src/compositor`, `src/backends`, `src/wayland`, `src/x11`, and
//! `src/frames` directories.

pub mod backends;
pub mod clutter;
pub mod compositor;
pub mod core;
pub mod frames;
pub mod math;
pub mod meta;
pub mod mtk;
pub mod wayland;
pub mod x11;
