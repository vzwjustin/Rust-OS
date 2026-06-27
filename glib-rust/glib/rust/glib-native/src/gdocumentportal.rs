//! GDocumentPortal matching `gio/gdocumentportal.h`.
//! Portal for document access. In this no_std port we model it with
//! a registry of document IDs and paths.
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// A document portal (`GDocumentPortal`).
pub struct DocumentPortal {
    documents: Mutex<BTreeMap<String, String>>,
    available: Mutex<bool>,
}

impl DocumentPortal {
    pub fn new() -> Self {
        Self {
            documents: Mutex::new(BTreeMap::new()),
            available: Mutex::new(false),
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn add_document(&self, doc_id: &str, path: &str) -> bool {
        if !*self.available.lock() {
            return false;
        }
        self.documents
            .lock()
            .insert(doc_id.to_string(), path.to_string());
        true
    }

    pub fn get_document_path(&self, doc_id: &str) -> Option<String> {
        self.documents.lock().get(doc_id).cloned()
    }

    pub fn remove_document(&self, doc_id: &str) -> bool {
        self.documents.lock().remove(doc_id).is_some()
    }

    pub fn document_count(&self) -> usize {
        self.documents.lock().len()
    }
}

impl Default for DocumentPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_get() {
        let p = DocumentPortal::new();
        p.set_available(true);
        assert!(p.add_document("doc1", "/home/user/file.txt"));
        assert_eq!(
            p.get_document_path("doc1"),
            Some("/home/user/file.txt".to_string())
        );
        assert_eq!(p.document_count(), 1);
    }

    #[test]
    fn test_unavailable() {
        let p = DocumentPortal::new();
        assert!(!p.add_document("doc1", "/path"));
    }
}
