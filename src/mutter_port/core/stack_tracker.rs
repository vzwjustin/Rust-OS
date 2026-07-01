//! Stack tracker for window compositing
//!
//! Ported from GNOME Mutter's src/core/stack-tracker.c.
//! Maintains accurate view of window stacking order by reconciling:
//! - Verified stack (confirmed by server)
//! - Queue of pending restack operations
//! - Predicted stack (verified + pending ops applied)

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackOpType {
    Add,
    Remove,
    RaiseAbove,
    LowerBelow,
}

#[derive(Debug, Clone, Copy)]
pub struct StackOp {
    pub op_type: StackOpType,
    pub serial: u64,
    pub window: WindowId,
    pub sibling: Option<WindowId>,
}

pub struct StackTracker {
    verified_stack: Vec<WindowId>,
    unverified_predictions: Vec<StackOp>,
    predicted_stack: Option<Vec<WindowId>>,
    xserver_serial: u64,
}

impl StackTracker {
    pub fn new() -> Self {
        StackTracker {
            verified_stack: Vec::new(),
            unverified_predictions: Vec::new(),
            predicted_stack: None,
            xserver_serial: 0,
        }
    }

    fn find_window(stack: &[WindowId], window: WindowId) -> Option<usize> {
        stack.iter().position(|&w| w == window)
    }

    fn move_window_above(
        stack: &mut Vec<WindowId>,
        window: WindowId,
        old_pos: usize,
        above_pos: Option<usize>,
    ) -> bool {
        let above_pos = match above_pos {
            Some(p) => p,
            None => return false,
        };

        if old_pos < above_pos {
            // Moving window upward in stack
            if old_pos == above_pos {
                return false;
            }
            let window = stack.remove(old_pos);
            stack.insert(above_pos, window);
            true
        } else if old_pos > above_pos + 1 {
            // Moving window downward in stack
            let window = stack.remove(old_pos);
            stack.insert(above_pos + 1, window);
            true
        } else {
            false
        }
    }

    fn apply_op(stack: &mut Vec<WindowId>, op: &StackOp) -> bool {
        match op.op_type {
            StackOpType::Add => {
                if Self::find_window(stack, op.window).is_some() {
                    return false;
                }
                stack.push(op.window);
                true
            }
            StackOpType::Remove => {
                if let Some(pos) = Self::find_window(stack, op.window) {
                    stack.remove(pos);
                    true
                } else {
                    false
                }
            }
            StackOpType::RaiseAbove => {
                if let Some(old_pos) = Self::find_window(stack, op.window) {
                    let above_pos = op.sibling.and_then(|sib| Self::find_window(stack, sib));
                    Self::move_window_above(stack, op.window, old_pos, above_pos)
                } else {
                    false
                }
            }
            StackOpType::LowerBelow => {
                if let Some(old_pos) = Self::find_window(stack, op.window) {
                    let above_pos = op.sibling.and_then(|sib| {
                        Self::find_window(stack, sib).and_then(|below_pos| {
                            if below_pos > 0 {
                                Some(below_pos - 1)
                            } else {
                                None
                            }
                        })
                    });
                    Self::move_window_above(stack, op.window, old_pos, above_pos)
                } else {
                    false
                }
            }
        }
    }

    pub fn record_add(&mut self, window: WindowId, serial: u64) {
        let op = StackOp {
            op_type: StackOpType::Add,
            serial,
            window,
            sibling: None,
        };
        self.apply_prediction(op);
    }

    pub fn record_remove(&mut self, window: WindowId, serial: u64) {
        let op = StackOp {
            op_type: StackOpType::Remove,
            serial,
            window,
            sibling: None,
        };
        self.apply_prediction(op);
    }

    pub fn record_raise_above(&mut self, window: WindowId, sibling: WindowId, serial: u64) {
        let op = StackOp {
            op_type: StackOpType::RaiseAbove,
            serial,
            window,
            sibling: Some(sibling),
        };
        self.apply_prediction(op);
    }

    pub fn record_lower_below(&mut self, window: WindowId, sibling: WindowId, serial: u64) {
        let op = StackOp {
            op_type: StackOpType::LowerBelow,
            serial,
            window,
            sibling: Some(sibling),
        };
        self.apply_prediction(op);
    }

    fn apply_prediction(&mut self, op: StackOp) {
        if op.serial == 0 && self.unverified_predictions.is_empty() {
            Self::apply_op(&mut self.verified_stack, &op);
        } else {
            self.unverified_predictions.push(op);
        }

        // Update or invalidate predicted stack
        if self.predicted_stack.is_none() {
            self.recompute_predicted_stack();
        } else if let Some(ref mut predicted) = self.predicted_stack {
            Self::apply_op(predicted, &op);
        }
    }

    fn recompute_predicted_stack(&mut self) {
        let mut predicted = self.verified_stack.clone();
        for op in &self.unverified_predictions {
            Self::apply_op(&mut predicted, op);
        }
        self.predicted_stack = Some(predicted);
    }

    pub fn get_stack(&mut self) -> &[WindowId] {
        if self.unverified_predictions.is_empty() {
            &self.verified_stack
        } else {
            if self.predicted_stack.is_none() {
                self.recompute_predicted_stack();
            }
            self.predicted_stack.as_ref().unwrap()
        }
    }

    pub fn event_received(&mut self, op: &StackOp) {
        if op.serial < self.xserver_serial {
            return;
        }

        // Apply queued ops older than this event with local-only flag
        while !self.unverified_predictions.is_empty() {
            let queued = self.unverified_predictions[0];
            if queued.serial >= op.serial {
                break;
            }
            Self::apply_op(&mut self.verified_stack, &queued);
            self.unverified_predictions.remove(0);
        }

        // Apply the received event
        Self::apply_op(&mut self.verified_stack, op);

        // Apply remaining queued ops that are now verified
        while !self.unverified_predictions.is_empty() {
            let queued = self.unverified_predictions[0];
            if queued.serial > op.serial {
                break;
            }
            Self::apply_op(&mut self.verified_stack, &queued);
            self.unverified_predictions.remove(0);
        }

        // Invalidate predicted stack; will recompute on next get_stack()
        self.predicted_stack = None;
    }

    pub fn get_verified_stack(&self) -> &[WindowId] {
        &self.verified_stack
    }

    pub fn get_pending_ops(&self) -> &[StackOp] {
        &self.unverified_predictions
    }

    pub fn set_xserver_serial(&mut self, serial: u64) {
        self.xserver_serial = serial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_window() {
        let mut tracker = StackTracker::new();
        let w1 = WindowId(1);
        tracker.record_add(w1, 0);
        let stack = tracker.get_stack();
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0], w1);
    }

    #[test]
    fn test_raise_above() {
        let mut tracker = StackTracker::new();
        let w1 = WindowId(1);
        let w2 = WindowId(2);
        tracker.record_add(w1, 0);
        tracker.record_add(w2, 0);
        tracker.record_raise_above(w1, w2, 0);
        let stack = tracker.get_stack();
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[1], w1); // w1 should be above w2
        assert_eq!(stack[0], w2);
    }

    #[test]
    fn test_pending_ops() {
        let mut tracker = StackTracker::new();
        let w1 = WindowId(1);
        tracker.record_add(w1, 1); // serial 1 means pending
        assert_eq!(tracker.get_pending_ops().len(), 1);
        assert!(tracker.get_verified_stack().is_empty());
        assert_eq!(tracker.get_stack().len(), 1); // But predicted includes it
    }
}
