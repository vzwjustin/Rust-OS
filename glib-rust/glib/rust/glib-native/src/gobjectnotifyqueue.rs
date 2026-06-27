//! `gobjectnotifyqueue.c` compatibility helper.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Default)]
pub struct ObjectNotifyQueue {
    names: Vec<String>,
}

impl ObjectNotifyQueue {
    #[must_use]
    pub fn new() -> Self {
        Self { names: Vec::new() }
    }

    pub fn push(&mut self, name: &str) {
        self.names.push(String::from(name));
    }

    #[must_use]
    pub fn drain(&mut self) -> Vec<String> {
        core::mem::take(&mut self.names)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}
