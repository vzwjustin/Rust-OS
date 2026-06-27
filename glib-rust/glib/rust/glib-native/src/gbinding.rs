//! Property binding model (`gbinding.c`).

use crate::gvalue::GValue;
use alloc::string::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingFlags(pub u32);

impl BindingFlags {
    pub const DEFAULT: Self = Self(0);
    pub const BIDIRECTIONAL: Self = Self(1 << 0);
    pub const SYNC_CREATE: Self = Self(1 << 1);
    pub const INVERT_BOOLEAN: Self = Self(1 << 2);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

pub type BindingTransformFunc = fn(&GValue) -> Option<GValue>;

#[derive(Clone)]
pub struct Binding {
    source_property: String,
    target_property: String,
    flags: BindingFlags,
    transform_to: Option<BindingTransformFunc>,
    transform_from: Option<BindingTransformFunc>,
    active: bool,
}

impl Binding {
    #[must_use]
    pub fn new(source_property: &str, target_property: &str, flags: BindingFlags) -> Self {
        Self {
            source_property: String::from(source_property),
            target_property: String::from(target_property),
            flags,
            transform_to: None,
            transform_from: None,
            active: true,
        }
    }

    #[must_use]
    pub fn new_full(
        source_property: &str,
        target_property: &str,
        flags: BindingFlags,
        transform_to: Option<BindingTransformFunc>,
        transform_from: Option<BindingTransformFunc>,
    ) -> Self {
        Self {
            transform_to,
            transform_from,
            ..Self::new(source_property, target_property, flags)
        }
    }

    #[must_use]
    pub fn source_property(&self) -> &str {
        &self.source_property
    }

    #[must_use]
    pub fn target_property(&self) -> &str {
        &self.target_property
    }

    #[must_use]
    pub fn flags(&self) -> BindingFlags {
        self.flags
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn unbind(&mut self) {
        self.active = false;
    }

    #[must_use]
    pub fn transform_to(&self, value: &GValue) -> Option<GValue> {
        self.transform_to
            .map_or_else(|| Some(value.clone()), |f| f(value))
    }

    #[must_use]
    pub fn transform_from(&self, value: &GValue) -> Option<GValue> {
        self.transform_from
            .map_or_else(|| Some(value.clone()), |f| f(value))
    }
}

#[must_use]
pub fn bind_property(source_property: &str, target_property: &str, flags: BindingFlags) -> Binding {
    Binding::new(source_property, target_property, flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_binding_metadata_and_unbinds() {
        let mut binding = bind_property("enabled", "visible", BindingFlags::SYNC_CREATE);
        assert_eq!(binding.source_property(), "enabled");
        assert_eq!(binding.target_property(), "visible");
        assert!(binding.flags().contains(BindingFlags::SYNC_CREATE));
        binding.unbind();
        assert!(!binding.is_active());
    }
}
