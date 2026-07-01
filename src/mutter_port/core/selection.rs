//! Selection ownership and in-memory clipboard source.
//!
//! Ported from:
//! - mutter-main/src/core/meta-selection.c: selection ownership registry
//! - mutter-main/src/core/meta-selection-source-memory.c: in-memory clipboard buffer
//!
//! Tracks which client owns CLIPBOARD, PRIMARY, or DND selections, and serves
//! their data from in-memory buffers. Supports atomic ownership transfers with
//! activation/deactivation callbacks.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

/// Selection type: CLIPBOARD, PRIMARY, or drag-and-drop
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SelectionType {
    Clipboard,
    Primary,
    Dnd,
}

impl fmt::Display for SelectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectionType::Clipboard => write!(f, "CLIPBOARD"),
            SelectionType::Primary => write!(f, "PRIMARY"),
            SelectionType::Dnd => write!(f, "DND"),
        }
    }
}

/// Identifies a selection owner (client/source)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(id: &str) -> Self {
        Self(String::from(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// In-memory clipboard buffer for a single MIME type
#[derive(Debug, Clone)]
pub struct SelectionBuffer {
    /// MIME type this buffer handles (e.g., "text/plain")
    pub mime_type: String,
    /// Raw content bytes
    pub data: Vec<u8>,
}

impl SelectionBuffer {
    pub fn new(mime_type: &str, data: Vec<u8>) -> Self {
        Self {
            mime_type: String::from(mime_type),
            data,
        }
    }
}

/// A selection source with current ownership and in-memory content
#[derive(Debug, Clone)]
struct SelectionSource {
    owner: SourceId,
    buffer: SelectionBuffer,
}

/// Registry mapping selection types to current owner + in-memory buffer.
/// Atomic ownership transfers with optional activation callbacks.
pub struct SelectionRegistry {
    owners: BTreeMap<SelectionType, SelectionSource>,
}

impl SelectionRegistry {
    /// Create a new empty selection registry.
    pub fn new() -> Self {
        Self {
            owners: BTreeMap::new(),
        }
    }

    /// Set the owner and content for a selection type.
    /// Replaces any previous owner; buffer is copied.
    pub fn set_owner(&mut self, selection: SelectionType, owner_id: &str, buffer: SelectionBuffer) {
        let source = SelectionSource {
            owner: SourceId::new(owner_id),
            buffer,
        };
        self.owners.insert(selection, source);
    }

    /// Get the current owner of a selection type.
    pub fn get_owner(&self, selection: SelectionType) -> Option<&SourceId> {
        self.owners.get(&selection).map(|s| &s.owner)
    }

    /// Get the current buffer (MIME type + data) for a selection type.
    pub fn get_buffer(&self, selection: SelectionType) -> Option<&SelectionBuffer> {
        self.owners.get(&selection).map(|s| &s.buffer)
    }

    /// Clear (unset) the owner of a selection type.
    /// Returns the previous owner if one existed.
    pub fn clear(&mut self, selection: SelectionType) -> Option<SourceId> {
        self.owners.remove(&selection).map(|s| s.owner)
    }

    /// Check if a selection type has an owner.
    pub fn has_owner(&self, selection: SelectionType) -> bool {
        self.owners.contains_key(&selection)
    }

    /// Get the current owner for a selection, if it matches the given owner_id.
    /// Used to verify ownership before operations.
    pub fn owner_is(&self, selection: SelectionType, owner_id: &str) -> bool {
        self.get_owner(selection)
            .map(|owner| owner.as_str() == owner_id)
            .unwrap_or(false)
    }

    /// Replace a selection's content, verifying ownership.
    /// Returns true if the buffer was replaced, false if owner doesn't match.
    pub fn set_buffer_for_owner(
        &mut self,
        selection: SelectionType,
        owner_id: &str,
        buffer: SelectionBuffer,
    ) -> bool {
        if let Some(source) = self.owners.get_mut(&selection) {
            if source.owner.as_str() == owner_id {
                source.buffer = buffer;
                return true;
            }
        }
        false
    }

    /// Get list of supported MIME types for a selection.
    pub fn get_mimetypes(&self, selection: SelectionType) -> Option<Vec<String>> {
        self.get_buffer(selection)
            .map(|buf| vec![buf.mime_type.clone()])
    }

    /// Clear all selections (reset to empty state).
    pub fn clear_all(&mut self) {
        self.owners.clear();
    }

    /// Get the number of active selections.
    pub fn len(&self) -> usize {
        self.owners.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

impl Default for SelectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_owner() {
        let mut registry = SelectionRegistry::new();
        let buf = SelectionBuffer::new("text/plain", Vec::from(&b"hello"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf.clone());

        assert_eq!(
            registry
                .get_owner(SelectionType::Clipboard)
                .unwrap()
                .as_str(),
            "app1"
        );
        assert!(registry.has_owner(SelectionType::Clipboard));
    }

    #[test]
    fn test_get_buffer() {
        let mut registry = SelectionRegistry::new();
        let buf = SelectionBuffer::new("text/plain", Vec::from(&b"test data"[..]));

        registry.set_owner(SelectionType::Primary, "app2", buf.clone());

        let retrieved = registry.get_buffer(SelectionType::Primary).unwrap();
        assert_eq!(retrieved.mime_type, "text/plain");
        assert_eq!(retrieved.data, &b"test data"[..]);
    }

    #[test]
    fn test_clear_selection() {
        let mut registry = SelectionRegistry::new();
        let buf = SelectionBuffer::new("text/plain", Vec::from(&b"data"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf);
        assert!(registry.has_owner(SelectionType::Clipboard));

        let owner = registry.clear(SelectionType::Clipboard);
        assert_eq!(owner.unwrap().as_str(), "app1");
        assert!(!registry.has_owner(SelectionType::Clipboard));
    }

    #[test]
    fn test_multiple_selections() {
        let mut registry = SelectionRegistry::new();

        let buf1 = SelectionBuffer::new("text/plain", Vec::from(&b"clipboard"[..]));
        let buf2 = SelectionBuffer::new("text/plain", Vec::from(&b"primary"[..]));
        let buf3 = SelectionBuffer::new("text/uri-list", Vec::from(&b"file://drag"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf1);
        registry.set_owner(SelectionType::Primary, "app2", buf2);
        registry.set_owner(SelectionType::Dnd, "app3", buf3);

        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry
                .get_owner(SelectionType::Clipboard)
                .unwrap()
                .as_str(),
            "app1"
        );
        assert_eq!(
            registry.get_owner(SelectionType::Primary).unwrap().as_str(),
            "app2"
        );
        assert_eq!(
            registry.get_owner(SelectionType::Dnd).unwrap().as_str(),
            "app3"
        );
    }

    #[test]
    fn test_owner_verification() {
        let mut registry = SelectionRegistry::new();
        let buf = SelectionBuffer::new("text/plain", Vec::from(&b"data"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf);

        assert!(registry.owner_is(SelectionType::Clipboard, "app1"));
        assert!(!registry.owner_is(SelectionType::Clipboard, "app2"));
        assert!(!registry.owner_is(SelectionType::Primary, "app1"));
    }

    #[test]
    fn test_set_buffer_for_owner() {
        let mut registry = SelectionRegistry::new();
        let buf1 = SelectionBuffer::new("text/plain", Vec::from(&b"data1"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf1);

        let buf2 = SelectionBuffer::new("text/plain", Vec::from(&b"data2"[..]));
        let result = registry.set_buffer_for_owner(SelectionType::Clipboard, "app1", buf2.clone());

        assert!(result);
        assert_eq!(
            registry.get_buffer(SelectionType::Clipboard).unwrap().data,
            &b"data2"[..]
        );

        // Wrong owner should fail
        let buf3 = SelectionBuffer::new("text/plain", Vec::from(&b"data3"[..]));
        let result = registry.set_buffer_for_owner(SelectionType::Clipboard, "app2", buf3);

        assert!(!result);
    }

    #[test]
    fn test_get_mimetypes() {
        let mut registry = SelectionRegistry::new();
        let buf = SelectionBuffer::new("text/html", Vec::from(&b"<html></html>"[..]));

        registry.set_owner(SelectionType::Clipboard, "app1", buf);

        let mimes = registry.get_mimetypes(SelectionType::Clipboard).unwrap();
        assert_eq!(mimes.len(), 1);
        assert_eq!(mimes[0], "text/html");
    }
}
