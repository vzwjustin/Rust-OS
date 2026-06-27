//! `gsourceclosure.c` compatibility facade.

pub use crate::gclosure::{Closure, ClosureCallback};

#[derive(Clone)]
pub struct SourceClosure {
    closure: Closure,
}

impl SourceClosure {
    #[must_use]
    pub fn new(closure: Closure) -> Self {
        Self { closure }
    }

    #[must_use]
    pub fn closure(&self) -> &Closure {
        &self.closure
    }
}
