use alloc::{boxed::Box, string::String, vec::Vec};

pub struct KeyboardA11y {
    // TODO: port KeyboardA11y from meta-keyboard-a11y.c
}

impl KeyboardA11y {
    pub fn new() -> Self {
        KeyboardA11y {}
    }
}

impl Default for KeyboardA11y {
    fn default() -> Self {
        Self::new()
    }
}
