//! `gclosure.c` compatibility facade.

use crate::gvalue::GValue;
use alloc::vec::Vec;

pub type ClosureCallback = fn(&[GValue]) -> Option<GValue>;

#[derive(Clone)]
pub struct Closure {
    callback: ClosureCallback,
    invalidated: bool,
}

impl Closure {
    #[must_use]
    pub fn new(callback: ClosureCallback) -> Self {
        Self {
            callback,
            invalidated: false,
        }
    }

    #[must_use]
    pub fn invoke(&self, params: &[GValue]) -> Option<GValue> {
        if self.invalidated {
            None
        } else {
            (self.callback)(params)
        }
    }

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    #[must_use]
    pub fn is_invalidated(&self) -> bool {
        self.invalidated
    }
}

#[must_use]
pub fn closure_invoke(closure: &Closure, params: Vec<GValue>) -> Option<GValue> {
    closure.invoke(&params)
}
