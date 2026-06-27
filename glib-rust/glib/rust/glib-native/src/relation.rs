//! Relation and tuples matching `grel.h` / `grel.c` (deprecated).
//!
//! A simple in-memory relation (table) supporting indexed lookups.
//! Deprecated in GLib 2.26. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;

/// A tuple in a relation.
pub type Tuple = Vec<String>;

/// A relation (`GRelation`).
///
/// A simple table with N fields per record. Supports indexing by field
/// for fast lookups. Deprecated in GLib 2.26.
pub struct Relation {
    fields: usize,
    records: Vec<Tuple>,
    /// Index: field_index -> (key -> record_indices)
    indices: BTreeMap<usize, BTreeMap<String, Vec<usize>>>,
}

impl Relation {
    /// Create a new relation with `fields` columns (`g_relation_new`).
    pub fn new(fields: usize) -> Self {
        Self {
            fields,
            records: Vec::new(),
            indices: BTreeMap::new(),
        }
    }

    /// Index a field for faster lookups (`g_relation_index`).
    ///
    /// In this simple implementation, all fields are indexed by string value.
    pub fn index(&mut self, field: usize) {
        if field >= self.fields {
            return;
        }
        let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, record) in self.records.iter().enumerate() {
            if let Some(val) = record.get(field) {
                index.entry(val.clone()).or_default().push(i);
            }
        }
        self.indices.insert(field, index);
    }

    /// Insert a record (`g_relation_insert`).
    pub fn insert(&mut self, record: Tuple) {
        if record.len() != self.fields {
            return;
        }
        let idx = self.records.len();
        for (&field, index) in &mut self.indices {
            if let Some(val) = record.get(field) {
                index.entry(val.clone()).or_default().push(idx);
            }
        }
        self.records.push(record);
    }

    /// Delete records matching a key in a field (`g_relation_delete`).
    ///
    /// Returns the number of deleted records.
    pub fn delete(&mut self, key: &str, field: usize) -> usize {
        let to_remove: Vec<usize> = if let Some(idx_map) = self.indices.get(&field) {
            idx_map.get(key).cloned().unwrap_or_default()
        } else {
            self.records
                .iter()
                .enumerate()
                .filter(|(_, r)| r.get(field).map(|s| s.as_str()) == Some(key))
                .map(|(i, _)| i)
                .collect()
        };
        let count = to_remove.len();
        let mut sorted = to_remove;
        sorted.sort_unstable();
        sorted.dedup();
        for &i in sorted.iter().rev() {
            self.records.remove(i);
        }
        let fields: Vec<usize> = self.indices.keys().copied().collect();
        self.indices.clear();
        for f in fields {
            self.index(f);
        }
        count
    }

    /// Select records matching a key in a field (`g_relation_select`).
    ///
    /// Returns the matching tuples.
    pub fn select(&self, key: &str, field: usize) -> Vec<&Tuple> {
        if let Some(index) = self.indices.get(&field) {
            if let Some(indices) = index.get(key) {
                return indices
                    .iter()
                    .filter_map(|&i| self.records.get(i))
                    .collect();
            }
        }
        // Fallback: linear scan
        self.records
            .iter()
            .filter(|r| r.get(field).map_or(false, |v| v == key))
            .collect()
    }

    /// Count records matching a key in a field (`g_relation_count`).
    pub fn count(&self, key: &str, field: usize) -> usize {
        self.select(key, field).len()
    }

    /// Check if a record exists (`g_relation_exists`).
    pub fn exists(&self, record: &Tuple) -> bool {
        self.records.iter().any(|r| r == record)
    }

    /// Get the number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` if the relation is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Get the number of fields.
    pub fn fields(&self) -> usize {
        self.fields
    }
}

/// Tuples result (`GTuples`).
///
/// A collection of matching tuples from a select operation.
pub struct Tuples {
    pub records: Vec<Tuple>,
}

impl Tuples {
    /// Get the number of tuples (`GTuples.len`).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Get a field value from a tuple (`g_tuples_index`).
    pub fn index(&self, index: usize, field: usize) -> Option<&String> {
        self.records.get(index).and_then(|r| r.get(field))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_relation() {
        let mut rel = Relation::new(3);
        rel.insert(vec!["Alice".to_owned(), "30".to_owned(), "NYC".to_owned()]);
        rel.insert(vec!["Bob".to_owned(), "25".to_owned(), "LA".to_owned()]);
        rel.insert(vec!["Carol".to_owned(), "30".to_owned(), "NYC".to_owned()]);

        assert_eq!(rel.len(), 3);
        assert_eq!(rel.fields(), 3);
    }

    #[test]
    fn index_and_select() {
        let mut rel = Relation::new(2);
        rel.insert(vec!["a".to_owned(), "1".to_owned()]);
        rel.insert(vec!["b".to_owned(), "2".to_owned()]);
        rel.insert(vec!["a".to_owned(), "3".to_owned()]);
        rel.index(0);

        let results = rel.select("a", 0);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn count() {
        let mut rel = Relation::new(2);
        rel.insert(vec!["x".to_owned(), "1".to_owned()]);
        rel.insert(vec!["x".to_owned(), "2".to_owned()]);
        rel.insert(vec!["y".to_owned(), "3".to_owned()]);
        rel.index(0);

        assert_eq!(rel.count("x", 0), 2);
        assert_eq!(rel.count("y", 0), 1);
        assert_eq!(rel.count("z", 0), 0);
    }

    #[test]
    fn delete() {
        let mut rel = Relation::new(2);
        rel.insert(vec!["a".to_owned(), "1".to_owned()]);
        rel.insert(vec!["b".to_owned(), "2".to_owned()]);
        rel.insert(vec!["a".to_owned(), "3".to_owned()]);
        rel.index(0);

        let deleted = rel.delete("a", 0);
        assert_eq!(deleted, 2);
        assert_eq!(rel.len(), 1);
    }

    #[test]
    fn exists() {
        let mut rel = Relation::new(2);
        rel.insert(vec!["a".to_owned(), "1".to_owned()]);
        assert!(rel.exists(&vec!["a".to_owned(), "1".to_owned()]));
        assert!(!rel.exists(&vec!["b".to_owned(), "2".to_owned()]));
    }

    #[test]
    fn tuples() {
        let t = Tuples {
            records: vec![
                vec!["a".to_owned(), "1".to_owned()],
                vec!["b".to_owned(), "2".to_owned()],
            ],
        };
        assert_eq!(t.len(), 2);
        assert_eq!(t.index(0, 0), Some(&"a".to_owned()));
        assert_eq!(t.index(1, 1), Some(&"2".to_owned()));
        assert_eq!(t.index(2, 0), None);
    }

    #[test]
    fn select_without_index() {
        let mut rel = Relation::new(2);
        rel.insert(vec!["a".to_owned(), "1".to_owned()]);
        rel.insert(vec!["b".to_owned(), "2".to_owned()]);
        // No index built - should fall back to linear scan
        let results = rel.select("b", 0);
        assert_eq!(results.len(), 1);
    }
}
