//! `gparamspecs.c` compatibility facade.
//!
//! Concrete `ParamSpec` constructors are implemented in [`crate::gparamspec`].
//! This module matches GLib's split file layout without duplicating the
//! constructor logic.

pub use crate::gparamspec::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_param_spec_types() {
        assert_ne!(ParamFlags::CONSTRUCT.0, 0);
        assert_ne!(ParamFlags::STATIC_NAME.0, 0);
    }
}
