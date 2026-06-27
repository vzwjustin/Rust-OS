//! `gparam.c` compatibility facade.
//!
//! The Rust port keeps parameter flags, specs, validation, and override
//! helpers in [`crate::gparamspec`]. This module preserves the upstream source
//! split for callers that expect a `gparam` module.

pub use crate::gparamspec::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_param_flags() {
        assert_ne!(ParamFlags::READABLE.0, 0);
        assert_ne!(ParamFlags::WRITABLE.0, 0);
    }
}
