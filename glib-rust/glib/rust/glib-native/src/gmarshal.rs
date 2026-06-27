//! `gmarshal.c` compatibility facade.

pub use crate::gclosure::{closure_invoke, Closure, ClosureCallback};

#[must_use]
pub fn marshal(
    closure: &Closure,
    params: &[crate::gvalue::GValue],
) -> Option<crate::gvalue::GValue> {
    closure.invoke(params)
}
