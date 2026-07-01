//! Port of GNOME mutter's `clutter/clutter-damage-history.{c,h}`.
//!
//! `ClutterDamageHistory` is a fixed-size ring buffer of recent damage
//! regions, used by the compositor's "swap throttle" / frame-scheduling
//! logic to look up the damage that occurred N frames ago.
//!
//! This is a faithful, complete port — the C source is small and has no
//! GObject or backend dependencies beyond `MtkRegion`, which is already
//! ported in `mtk::region`.
//!
//! # What's ported
//!
//! - The `DAMAGE_HISTORY_LENGTH = 0x10` (16) ring size.
//! - The `ClutterDamageHistory` struct (`damages[16]`, `index`) as a
//!   `DamageHistory` owning 16 `Option<Region>` slots.
//! - `clutter_damage_history_new` / `_free` (Drop covers free).
//! - `clutter_damage_history_is_age_valid`: age in `[1, 16)` and the
//!   looked-up slot is non-empty.
//! - `clutter_damage_history_record`: replace the current slot with a copy
//!   of the damage region (dropping the old one, matching
//!   `g_clear_pointer` + `mtk_region_copy`).
//! - `clutter_damage_history_step`: advance the index by one (wrapping via
//!   bitmask, matching `step_damage_index`).
//! - `clutter_damage_history_lookup`: return the slot `age` frames back
//!   (wrapping via `step_damage_index(index, -age)`).
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::super::mtk::region::Region;

/// `DAMAGE_HISTORY_LENGTH` — the ring buffer size (16). A power of two so
/// the index wrap is a bitmask, matching `step_damage_index`.
pub const DAMAGE_HISTORY_LENGTH: usize = 0x10;

/// Port of `ClutterDamageHistory`.
#[derive(Debug)]
pub struct DamageHistory {
    damages: [Option<Region>; DAMAGE_HISTORY_LENGTH],
    index: usize,
}

impl Default for DamageHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl DamageHistory {
    /// `clutter_damage_history_new`.
    pub fn new() -> Self {
        // `Option<Region>::default()` is `None`, matching `g_new0`.
        const NONE: Option<Region> = None;
        DamageHistory {
            damages: [NONE; DAMAGE_HISTORY_LENGTH],
            index: 0,
        }
    }

    /// `clutter_damage_history_is_age_valid`: `age` must be in
    /// `[1, DAMAGE_HISTORY_LENGTH)` and the looked-up slot must be
    /// non-empty.
    pub fn is_age_valid(&self, age: i32) -> bool {
        if age >= DAMAGE_HISTORY_LENGTH as i32 || age < 1 {
            return false;
        }
        self.lookup(age).is_some()
    }

    /// `clutter_damage_history_record`: replace the current slot with a
    /// copy of `damage` (dropping the previous content, matching
    /// `g_clear_pointer` + `mtk_region_copy`).
    pub fn record(&mut self, damage: &Region) {
        self.damages[self.index] = Some(damage.copy());
    }

    /// `clutter_damage_history_step`: advance the write index by one
    /// (wrapping via bitmask).
    pub fn step(&mut self) {
        self.index = step_damage_index(self.index, 1);
    }

    /// `clutter_damage_history_lookup`: return the damage region `age`
    /// frames back, or `None` if that slot is empty. `age` is clamped to
    /// the valid range by the caller (see `is_age_valid`); out-of-range
    /// ages simply wrap via the bitmask and may return `None`.
    pub fn lookup(&self, age: i32) -> Option<&Region> {
        let slot = step_damage_index(self.index, -age);
        self.damages[slot].as_ref()
    }
}

/// `step_damage_index`: `(current + diff) & (LENGTH - 1)`. The C version
/// uses `int` arithmetic with a bitmask; here `diff` is `i32` to allow
/// negative offsets (lookup), and the result is wrapped into `usize` via
/// two's-complement of the bitmask.
fn step_damage_index(current: usize, diff: i32) -> usize {
    // Add `diff` modulo `DAMAGE_HISTORY_LENGTH`. Using i64 to avoid overflow
    // on negative `diff`, then mask to the power-of-two length.
    let raw = current as i64 + diff as i64;
    // `rem_euclid` gives a non-negative remainder, which for a power-of-two
    // length matches the C bitmask wrap for both positive and negative diff.
    let rem = raw.rem_euclid(DAMAGE_HISTORY_LENGTH as i64);
    rem as usize
}

#[cfg(test)]
mod tests {
    use super::super::super::mtk::rectangle::Rectangle;
    use super::*;

    fn region_with(rect: Rectangle) -> Region {
        let mut r = Region::create();
        r.union_rectangle(&rect);
        r
    }

    #[test]
    fn new_starts_empty() {
        let h = DamageHistory::new();
        for age in 1..DAMAGE_HISTORY_LENGTH as i32 {
            assert!(!h.is_age_valid(age), "age {} should be invalid", age);
        }
    }

    #[test]
    fn record_then_step_then_lookup() {
        let mut h = DamageHistory::new();
        let r1 = region_with(Rectangle::new(0, 0, 10, 10));
        h.record(&r1);
        h.step();
        // age 1 -> the slot we just wrote (one step back).
        assert!(h.is_age_valid(1));
        let looked = h.lookup(1).unwrap();
        assert_eq!(looked.num_rectangles(), 1);
        assert_eq!(looked.get_rectangle(0), Rectangle::new(0, 0, 10, 10));
    }

    #[test]
    fn record_overwrites_current_slot() {
        let mut h = DamageHistory::new();
        let r1 = region_with(Rectangle::new(0, 0, 10, 10));
        let r2 = region_with(Rectangle::new(5, 5, 20, 20));
        h.record(&r1);
        h.record(&r2); // overwrites slot 0 without stepping
                       // age 0 isn't valid (lookup uses negative offset from index, and
                       // index hasn't stepped), so look up via the current slot directly:
                       // after two records at index 0, lookup(0) returns slot 0.
        let looked = h.lookup(0).unwrap();
        assert_eq!(looked.get_rectangle(0), Rectangle::new(5, 5, 20, 20));
    }

    #[test]
    fn ring_wraps_after_length_steps() {
        let mut h = DamageHistory::new();
        // Fill the ring with distinct regions and step each time.
        for i in 0..DAMAGE_HISTORY_LENGTH {
            let r = region_with(Rectangle::new(i as i32, 0, 1, 1));
            h.record(&r);
            h.step();
        }
        // After LENGTH steps, index has wrapped back to 0. The most recent
        // record (age 1) is the last one written, at slot LENGTH-1.
        let most_recent = h.lookup(1).unwrap();
        assert_eq!(
            most_recent.get_rectangle(0),
            Rectangle::new((DAMAGE_HISTORY_LENGTH - 1) as i32, 0, 1, 1)
        );
        // age == LENGTH is invalid (out of range).
        assert!(!h.is_age_valid(DAMAGE_HISTORY_LENGTH as i32));
        // age 0 is invalid (< 1).
        assert!(!h.is_age_valid(0));
    }

    #[test]
    fn step_damage_index_wraps_negative() {
        // length 16: index 0, diff -1 -> slot 15.
        assert_eq!(step_damage_index(0, -1), 15);
        // index 3, diff -4 -> slot 15.
        assert_eq!(step_damage_index(3, -4), 15);
        // index 0, diff +1 -> slot 1.
        assert_eq!(step_damage_index(0, 1), 1);
        // index 15, diff +1 -> slot 0 (wrap).
        assert_eq!(step_damage_index(15, 1), 0);
    }
}
