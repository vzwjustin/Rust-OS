//! Port of GNOME mutter's `clutter/clutter-keyframe-transition.{c,h}`.
//!
//! `ClutterKeyframeTransition` animates through multiple keyframes, each
//! with its own easing mode for the segment that follows it.
//!
//! # What's ported
//!
//! - `Keyframe` struct: `(key, value, mode)`.
//! - Keyframe scheduling: `compute_keyframe_value` finds the bracketing
//!   segment, computes local progress, applies segment easing, lerps.
//! - `set_keyframes` / `add_keyframe` / `clear_keyframes` / `keyframes`.
//! - C-style setters: `set_keys`, `set_values`, `set_modes`.
//! - `advance` with keyframe-interpolated value; falls back to linear
//!   interval interpolation when no keyframes are set.

#![allow(dead_code)]

use alloc::vec::Vec;

use super::super::easing::{easing_for_mode, AnimationMode};
use super::super::interval::Interval;
use super::super::timeline::Direction;
use super::transition::{AnimatableHandle, Transition, TransitionResult};

/// A single keyframe: normalized time, value, and easing mode for the
/// segment starting at this keyframe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub key: f64,
    pub value: f64,
    pub mode: AnimationMode,
}

impl Keyframe {
    pub fn new(key: f64, value: f64, mode: AnimationMode) -> Self {
        Keyframe { key, value, mode }
    }
}

/// Result of `KeyframeTransition::advance`.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeResult {
    pub markers: Vec<super::super::timeline::Marker>,
    pub completed: bool,
    pub progress: f64,
    pub value: Option<f64>,
}

/// Port of `ClutterKeyframeTransition`.
#[derive(Debug, Clone)]
pub struct KeyframeTransition {
    transition: Transition,
    keyframes: Vec<Keyframe>,
}

impl KeyframeTransition {
    pub fn new(duration_ms: u32) -> Self {
        KeyframeTransition { transition: Transition::new(duration_ms), keyframes: Vec::new() }
    }

    pub fn new_with_keyframes(duration_ms: u32, keyframes: Vec<Keyframe>) -> Self {
        let mut kt = Self::new(duration_ms);
        kt.set_keyframes(keyframes);
        kt
    }

    pub fn set_keyframes(&mut self, mut keyframes: Vec<Keyframe>) {
        if keyframes.is_empty() { self.keyframes.clear(); return; }
        keyframes.sort_by(|a, b| a.key.partial_cmp(&b.key).unwrap_or(core::cmp::Ordering::Equal));
        if keyframes[0].key > 0.0 {
            let first = keyframes[0];
            keyframes.insert(0, Keyframe::new(0.0, first.value, first.mode));
        }
        let last_idx = keyframes.len() - 1;
        if keyframes[last_idx].key < 1.0 {
            let last = keyframes[last_idx];
            keyframes.push(Keyframe::new(1.0, last.value, AnimationMode::Linear));
        }
        self.keyframes = keyframes;
    }

    pub fn add_keyframe(&mut self, keyframe: Keyframe) {
        self.keyframes.push(keyframe);
        self.keyframes.sort_by(|a, b| a.key.partial_cmp(&b.key).unwrap_or(core::cmp::Ordering::Equal));
    }

    pub fn clear_keyframes(&mut self) { self.keyframes.clear(); }
    pub fn keyframes(&self) -> &[Keyframe] { &self.keyframes }
    pub fn n_keyframes(&self) -> usize { self.keyframes.len() }

    pub fn set_keys(&mut self, keys: &[f64]) {
        if keys.len() != self.keyframes.len() { return; }
        for (kf, &k) in self.keyframes.iter_mut().zip(keys.iter()) {
            kf.key = k.clamp(0.0, 1.0);
        }
        self.keyframes.sort_by(|a, b| a.key.partial_cmp(&b.key).unwrap_or(core::cmp::Ordering::Equal));
    }

    pub fn set_values(&mut self, values: &[f64]) {
        if values.len() != self.keyframes.len() { return; }
        for (kf, &v) in self.keyframes.iter_mut().zip(values.iter()) { kf.value = v; }
    }

    pub fn set_modes(&mut self, modes: &[AnimationMode]) {
        if modes.len() != self.keyframes.len() { return; }
        for (kf, &m) in self.keyframes.iter_mut().zip(modes.iter()) { kf.mode = m; }
    }

    pub fn set_interval(&mut self, interval: Interval) { self.transition.set_interval(interval); }
    pub fn interval(&self) -> Option<Interval> { self.transition.interval() }
    pub fn set_animatable(&mut self, handle: AnimatableHandle) { self.transition.set_animatable(handle); }
    pub fn animatable(&self) -> AnimatableHandle { self.transition.animatable() }
    pub fn set_easing_mode(&mut self, mode: AnimationMode) { self.transition.set_easing_mode(mode); }
    pub fn easing_mode(&self) -> AnimationMode { self.transition.easing_mode() }
    pub fn set_remove_on_complete(&mut self, remove: bool) { self.transition.set_remove_on_complete(remove); }
    pub fn remove_on_complete(&self) -> bool { self.transition.remove_on_complete() }
    pub fn start(&mut self) { self.transition.start(); }
    pub fn stop(&mut self) { self.transition.stop(); }
    pub fn pause(&mut self) { self.transition.pause(); }
    pub fn rewind(&mut self) { self.transition.rewind(); }

    pub fn advance(&mut self, delta_ms: u32) -> KeyframeResult {
        if self.keyframes.is_empty() {
            let r = self.transition.advance(delta_ms);
            return KeyframeResult { markers: r.markers, completed: r.completed, progress: r.progress, value: r.value };
        }
        let (markers, completed) = self.transition.advance_timeline(delta_ms);
        let progress = self.transition.raw_progress();
        let value = compute_keyframe_value(&self.keyframes, progress);
        if completed { self.transition.on_completed(); }
        KeyframeResult { markers, completed, progress, value: Some(value) }
    }

    pub fn duration(&self) -> u32 { self.transition.duration() }
    pub fn set_duration(&mut self, msecs: u32) { self.transition.set_duration(msecs); }
    pub fn is_playing(&self) -> bool { self.transition.is_playing() }
    pub fn direction(&self) -> Direction { self.transition.direction() }
    pub fn set_direction(&mut self, direction: Direction) { self.transition.set_direction(direction); }
    pub fn repeat_count(&self) -> i32 { self.transition.repeat_count() }
    pub fn set_repeat_count(&mut self, count: i32) { self.transition.set_repeat_count(count); }
    pub fn auto_reverse(&self) -> bool { self.transition.auto_reverse() }
    pub fn set_auto_reverse(&mut self, enabled: bool) { self.transition.set_auto_reverse(enabled); }
    pub fn elapsed_time(&self) -> u32 { self.transition.elapsed_time() }
    pub fn inner(&self) -> &Transition { &self.transition }
    pub fn inner_mut(&mut self) -> &mut Transition { &mut self.transition }
}

/// Compute the interpolated value at `progress` using the keyframe schedule.
pub fn compute_keyframe_value(keyframes: &[Keyframe], progress: f64) -> f64 {
    if keyframes.is_empty() { return 0.0; }
    if keyframes.len() == 1 { return keyframes[0].value; }
    let progress = progress.clamp(0.0, 1.0);
    let mut index = keyframes.len() - 2;
    for i in 0..keyframes.len() - 1 {
        if keyframes[i].key <= progress && progress < keyframes[i + 1].key {
            index = i;
            break;
        }
    }
    let k0 = keyframes[index].key;
    let k1 = keyframes[index + 1].key;
    let v0 = keyframes[index].value;
    let v1 = keyframes[index + 1].value;
    let mode = keyframes[index].mode;
    let segment_size = k1 - k0;
    if segment_size <= 0.0 { return v1; }
    let local_progress = (progress - k0) / segment_size;
    let eased = easing_for_mode(mode, local_progress, 1.0);
    v0 + (v1 - v0) * eased
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_keyframe_value_two_keyframes_linear() {
        let keyframes = vec![
            Keyframe::new(0.0, 0.0, AnimationMode::Linear),
            Keyframe::new(1.0, 100.0, AnimationMode::Linear),
        ];
        let v = compute_keyframe_value(&keyframes, 0.5);
        assert!((v - 50.0).abs() < 1e-10);
    }

    #[test]
    fn compute_keyframe_value_three_keyframes() {
        let keyframes = vec![
            Keyframe::new(0.0, 0.0, AnimationMode::Linear),
            Keyframe::new(0.5, 50.0, AnimationMode::Linear),
            Keyframe::new(1.0, 100.0, AnimationMode::Linear),
        ];
        assert!((compute_keyframe_value(&keyframes, 0.25) - 25.0).abs() < 1e-10);
        assert!((compute_keyframe_value(&keyframes, 0.75) - 75.0).abs() < 1e-10);
    }

    #[test]
    fn advance_with_keyframes() {
        let mut kt = KeyframeTransition::new_with_keyframes(100, vec![
            Keyframe::new(0.0, 0.0, AnimationMode::Linear),
            Keyframe::new(1.0, 100.0, AnimationMode::Linear),
        ]);
        kt.start();
        let r = kt.advance(50);
        assert!((r.value.unwrap() - 50.0).abs() < 1e-10);
    }
}
