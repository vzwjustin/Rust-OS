//! Port of GNOME mutter's `clutter/clutter-transition-group.{c,h}`.
//!
//! `ClutterTransitionGroup` manages a collection of `ClutterTransition`
//! objects driven by a single shared timeline.
//!
//! # What's ported
//!
//! - `TransitionGroup` struct: owns a `Timeline` and a `Vec<Transition>`.
//! - `add_transition` / `remove_transition`.
//! - `advance` — advances the group's timeline, then all children.
//! - `start` / `stop` / `pause` / `rewind` — control group + children.
//! - Timeline delegation with propagation to children.

#![allow(dead_code)]

use alloc::vec::Vec;

use super::super::timeline::{Direction, Marker, Timeline};
use super::transition::{Transition, TransitionResult};

/// Result of `TransitionGroup::advance`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupResult {
    pub markers: Vec<Marker>,
    pub completed: bool,
    pub progress: f64,
    pub child_results: Vec<TransitionResult>,
}

/// Port of `ClutterTransitionGroup`.
#[derive(Debug, Clone)]
pub struct TransitionGroup {
    timeline: Timeline,
    transitions: Vec<Transition>,
}

impl TransitionGroup {
    pub fn new(duration_ms: u32) -> Self {
        TransitionGroup { timeline: Timeline::new(duration_ms), transitions: Vec::new() }
    }

    pub fn add_transition(&mut self, mut transition: Transition) {
        transition.set_duration(self.timeline.duration());
        self.transitions.push(transition);
    }

    pub fn remove_transition(&mut self, index: usize) -> Option<Transition> {
        if index < self.transitions.len() { Some(self.transitions.remove(index)) } else { None }
    }

    pub fn transitions(&self) -> &[Transition] { &self.transitions }
    pub fn transitions_mut(&mut self) -> &mut [Transition] { &mut self.transitions }
    pub fn len(&self) -> usize { self.transitions.len() }
    pub fn is_empty(&self) -> bool { self.transitions.is_empty() }

    pub fn start(&mut self) {
        self.timeline.start();
        for t in &mut self.transitions { t.start(); }
    }

    pub fn stop(&mut self) {
        self.timeline.stop();
        for t in &mut self.transitions { t.stop(); }
    }

    pub fn pause(&mut self) {
        self.timeline.pause();
        for t in &mut self.transitions { t.pause(); }
    }

    pub fn rewind(&mut self) {
        self.timeline.rewind();
        for t in &mut self.transitions { t.rewind(); }
    }

    pub fn advance(&mut self, delta_ms: u32) -> GroupResult {
        let (markers, completed) = self.timeline.advance(delta_ms);
        let progress = self.timeline.progress();
        let child_results: Vec<TransitionResult> = self.transitions.iter_mut().map(|t| t.advance(delta_ms)).collect();
        GroupResult { markers, completed, progress, child_results }
    }

    pub fn duration(&self) -> u32 { self.timeline.duration() }
    pub fn set_duration(&mut self, msecs: u32) {
        self.timeline.set_duration(msecs);
        for t in &mut self.transitions { t.set_duration(msecs); }
    }
    pub fn is_playing(&self) -> bool { self.timeline.is_playing() }
    pub fn direction(&self) -> Direction { self.timeline.direction() }
    pub fn set_direction(&mut self, direction: Direction) {
        self.timeline.set_direction(direction);
        for t in &mut self.transitions { t.set_direction(direction); }
    }
    pub fn repeat_count(&self) -> i32 { self.timeline.repeat_count() }
    pub fn set_repeat_count(&mut self, count: i32) {
        self.timeline.set_repeat_count(count);
        for t in &mut self.transitions { t.set_repeat_count(count); }
    }
    pub fn auto_reverse(&self) -> bool { self.timeline.auto_reverse() }
    pub fn set_auto_reverse(&mut self, enabled: bool) {
        self.timeline.set_auto_reverse(enabled);
        for t in &mut self.transitions { t.set_auto_reverse(enabled); }
    }
    pub fn elapsed_time(&self) -> u32 { self.timeline.elapsed_time() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::interval::Interval;

    #[test]
    fn advance_drives_all_children() {
        let mut g = TransitionGroup::new(100);
        g.add_transition(Transition::new_with_interval(100, Interval::new(0.0, 10.0)));
        g.add_transition(Transition::new_with_interval(100, Interval::new(0.0, 20.0)));
        g.start();
        let r = g.advance(50);
        assert_eq!(r.child_results.len(), 2);
        assert!((r.child_results[0].value.unwrap() - 5.0).abs() < 1e-10);
        assert!((r.child_results[1].value.unwrap() - 10.0).abs() < 1e-10);
    }
}
