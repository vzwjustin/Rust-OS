//! Binding group support (`gbindinggroup.c`).

use crate::gbinding::{Binding, BindingFlags};
use alloc::vec::Vec;

#[derive(Clone, Default)]
pub struct BindingGroup {
    bindings: Vec<Binding>,
}

impl BindingGroup {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn bind(&mut self, source_property: &str, target_property: &str, flags: BindingFlags) {
        self.bindings
            .push(Binding::new(source_property, target_property, flags));
    }

    pub fn add_binding(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    pub fn clear(&mut self) {
        self.bindings.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
}
