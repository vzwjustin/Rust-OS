//! Deprecated `GValueArray` container (`gvaluearray.c`).
//!
//! GLib keeps this API for compatibility. New code should prefer ordinary
//! arrays, but bindings and older GObject paths still expect the basic
//! append/prepend/insert/remove/sort surface.

use crate::gvalue::GValue;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// Comparison callback used by `GValueArray` sorting helpers.
pub type ValueCompareFunc = fn(&GValue, &GValue) -> Ordering;

/// A growable array of `GValue`s.
#[derive(Clone, Default)]
pub struct GValueArray {
    values: Vec<GValue>,
}

impl GValueArray {
    /// Create a value array with preallocated capacity (`g_value_array_new`).
    #[must_use]
    pub fn new(n_prealloced: usize) -> Self {
        Self {
            values: Vec::with_capacity(n_prealloced),
        }
    }

    /// Number of values currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true when the array has no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Immutable access to the raw value slice.
    #[must_use]
    pub fn as_slice(&self) -> &[GValue] {
        &self.values
    }

    /// Mutable access to the raw value slice.
    pub fn as_mut_slice(&mut self) -> &mut [GValue] {
        &mut self.values
    }

    /// Get a value by index (`g_value_array_get_nth`).
    #[must_use]
    pub fn get_nth(&self, index: usize) -> Option<&GValue> {
        self.values.get(index)
    }

    /// Get a mutable value by index.
    pub fn get_nth_mut(&mut self, index: usize) -> Option<&mut GValue> {
        self.values.get_mut(index)
    }

    /// Insert a copy of `value` at `index` (`g_value_array_insert`).
    ///
    /// Indexes past the end append, matching GLib's compatibility behavior.
    pub fn insert(&mut self, index: usize, value: &GValue) -> &mut Self {
        let index = index.min(self.values.len());
        self.values.insert(index, value.clone());
        self
    }

    /// Append a copy of `value` (`g_value_array_append`).
    pub fn append(&mut self, value: &GValue) -> &mut Self {
        self.values.push(value.clone());
        self
    }

    /// Prepend a copy of `value` (`g_value_array_prepend`).
    pub fn prepend(&mut self, value: &GValue) -> &mut Self {
        self.values.insert(0, value.clone());
        self
    }

    /// Remove a value by index (`g_value_array_remove`).
    pub fn remove(&mut self, index: usize) -> Option<GValue> {
        if index < self.values.len() {
            Some(self.values.remove(index))
        } else {
            None
        }
    }

    /// Return a deep copy of the value array (`g_value_array_copy`).
    #[must_use]
    pub fn copy(&self) -> Self {
        self.clone()
    }

    /// Sort in place with a `GValue` comparator (`g_value_array_sort`).
    pub fn sort(&mut self, compare_func: ValueCompareFunc) -> &mut Self {
        self.values.sort_by(compare_func);
        self
    }
}

/// Create a value array with preallocated capacity.
#[must_use]
pub fn value_array_new(n_prealloced: usize) -> GValueArray {
    GValueArray::new(n_prealloced)
}

/// Return a copy of an array.
#[must_use]
pub fn value_array_copy(value_array: &GValueArray) -> GValueArray {
    value_array.copy()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn int_value(value: i32) -> GValue {
        let mut v = GValue::new();
        v.set_int(value);
        v
    }

    #[test]
    fn inserts_appends_and_removes_values() {
        let one = int_value(1);
        let two = int_value(2);
        let three = int_value(3);
        let mut array = GValueArray::new(2);

        array.append(&two).prepend(&one).insert(99, &three);

        assert_eq!(array.len(), 3);
        assert_eq!(array.get_nth(0).unwrap().get_int(), 1);
        assert_eq!(array.get_nth(1).unwrap().get_int(), 2);
        assert_eq!(array.get_nth(2).unwrap().get_int(), 3);
        assert_eq!(array.remove(1).unwrap().get_int(), 2);
        assert_eq!(array.len(), 2);
        assert!(array.remove(99).is_none());
    }

    #[test]
    fn copies_and_sorts_values() {
        let mut array = GValueArray::new(0);
        array.append(&int_value(3));
        array.append(&int_value(1));
        array.append(&int_value(2));

        let copy = value_array_copy(&array);
        assert_eq!(copy.len(), 3);

        array.sort(|a, b| a.get_int().cmp(&b.get_int()));
        let sorted: Vec<i32> = array.as_slice().iter().map(GValue::get_int).collect();
        assert_eq!(sorted, [1, 2, 3]);
    }
}
