//! Type module support (`gtypemodule.c`).

use alloc::string::String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeModule {
    name: String,
    loaded: bool,
    use_count: usize,
}

impl TypeModule {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            loaded: false,
            use_count: 0,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn use_module(&mut self) -> bool {
        self.use_count += 1;
        self.loaded = true;
        true
    }

    pub fn unuse_module(&mut self) {
        self.use_count = self.use_count.saturating_sub(1);
        if self.use_count == 0 {
            self.loaded = false;
        }
    }

    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    #[must_use]
    pub fn use_count(&self) -> usize {
        self.use_count
    }
}
