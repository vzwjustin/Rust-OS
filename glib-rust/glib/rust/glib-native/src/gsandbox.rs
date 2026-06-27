//! GSandbox matching `gio/gsandbox.h`.
//! Sandbox detection. In this no_std port we model it with a
//! sandbox type enum and detection state.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// Sandbox type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxType {
    None,
    Flatpak,
    Snap,
    Other,
}

/// Sandbox detection (`GSandbox`).
pub struct Sandbox {
    sandbox_type: Mutex<SandboxType>,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            sandbox_type: Mutex::new(SandboxType::None),
        }
    }

    pub fn get_type(&self) -> SandboxType {
        *self.sandbox_type.lock()
    }
    pub fn set_type(&self, t: SandboxType) {
        *self.sandbox_type.lock() = t;
    }
    pub fn is_sandboxed(&self) -> bool {
        *self.sandbox_type.lock() != SandboxType::None
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_sandboxed() {
        let s = Sandbox::new();
        assert!(!s.is_sandboxed());
    }

    #[test]
    fn test_flatpak() {
        let s = Sandbox::new();
        s.set_type(SandboxType::Flatpak);
        assert!(s.is_sandboxed());
        assert_eq!(s.get_type(), SandboxType::Flatpak);
    }
}
