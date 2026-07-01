//! Shared enumerations ported from GNOME mutter's
//! `clutter/clutter-enums.h` that are needed by more than one clutter
//! submodule (layout managers, constraints, effects, events).
//!
//! `ActorAlign` and `RequestMode` already live in `actor.rs` (they're
//! consumed by `ActorCommon`); this module holds the remaining enums
//! consumed across modules. Values match the C numbering where that
//! numbering is observable (bitfields, persisted values, or values
//! interchanged with Wayland/evdev), and are plain sequential Rust enums
//! otherwise.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

/// `ClutterOrientation` — horizontal vs vertical. Used by box layout,
/// constraints, and `needs_expand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum Orientation {
    /// `CLUTTER_ORIENTATION_HORIZONTAL`.
    #[default]
    Horizontal = 0,
    /// `CLUTTER_ORIENTATION_VERTICAL`.
    Vertical = 1,
}

/// `ClutterAlignAxis` — which axis `ClutterAlignConstraint` maintains.
/// Values match `clutter-enums.h`: `X_AXIS=0, Y_AXIS=1, BOTH=2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum AlignAxis {
    #[default]
    XAxis = 0,
    YAxis = 1,
    Both = 2,
}

/// `ClutterBindCoordinate` — which property `ClutterBindConstraint` binds.
/// Values match `clutter-enums.h`:
/// `X=0, Y=1, WIDTH=2, HEIGHT=3, POSITION=4, SIZE=5, ALL=6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum BindCoordinate {
    #[default]
    X = 0,
    Y = 1,
    Width = 2,
    Height = 3,
    Position = 4,
    Size = 5,
    All = 6,
}

/// `ClutterEffectPaintFlags` — flags passed to `Effect::paint`.
/// Bitfield; values match `clutter-enums.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EffectPaintFlags(pub u32);

impl EffectPaintFlags {
    /// `CLUTTER_EFFECT_PAINT_ACTOR_DIRTY` — the actor (or a child) queued a
    /// redraw since the last paint, so the effect can't use a cached image.
    pub const ACTOR_DIRTY: Self = Self(1 << 0);
    /// `CLUTTER_EFFECT_PAINT_BYPASS_EFFECT` — skip this effect for the frame
    /// but still paint the actor.
    pub const BYPASS_EFFECT: Self = Self(1 << 1);
    pub const NONE: Self = Self(0);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// `ClutterTextDirection` — text direction for the RTL flip in
/// `allocate_align_fill`. Only the two directions are needed by the ported
/// layout code; the C enum also has a `NONE` default which behaves as LTR
/// for the `1.0 - x_align` flip, so `Default` is `Ltr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum TextDirection {
    #[default]
    Ltr = 0,
    Rtl = 1,
}

/// `ClutterEventPhase` (clutter-enums.h) — when an action handles an event
/// relative to the target actor. Values match the C numbering:
/// `CAPTURE=0, TARGET=1, BUBBLE=2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum EventPhase {
    /// `CLUTTER_PHASE_CAPTURE` — action runs on the event during the
    /// capture phase (top-down, before the target).
    #[default]
    Capture = 0,
    /// `CLUTTER_PHASE_TARGET` — action runs at the target actor.
    Target = 1,
    /// `CLUTTER_PHASE_BUBBLE` — action runs during the bubble phase
    /// (bottom-up, after the target).
    Bubble = 2,
}

/// `ClutterSnapEdge`, used by `SnapConstraint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum SnapEdge {
    #[default]
    Top = 0,
    Right = 1,
    Bottom = 2,
    Left = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_flags_are_bitfield() {
        let f = EffectPaintFlags::ACTOR_DIRTY;
        assert!(f.contains(EffectPaintFlags::ACTOR_DIRTY));
        assert!(!f.contains(EffectPaintFlags::BYPASS_EFFECT));
        let both =
            EffectPaintFlags(EffectPaintFlags::ACTOR_DIRTY.0 | EffectPaintFlags::BYPASS_EFFECT.0);
        assert!(both.contains(EffectPaintFlags::ACTOR_DIRTY));
        assert!(both.contains(EffectPaintFlags::BYPASS_EFFECT));
    }
}
