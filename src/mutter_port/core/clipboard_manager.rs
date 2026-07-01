//! Clipboard manager ported from GNOME Mutter's src/core/meta-clipboard-manager.c
//!
//! Manages clipboard ownership, mimetype matching, and clipboard persistence
//! across workspace switches and compositor restarts.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-clipboard-manager.c

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Maximum size for text clipboard data (4MB)
const MAX_TEXT_SIZE: usize = 4 * 1024 * 1024;

/// Maximum size for image clipboard data (200MB)
const MAX_IMAGE_SIZE: usize = 200 * 1024 * 1024;

/// Mimetype with preference priority and max transfer size
#[derive(Debug, Clone)]
struct SupportedMimetype {
    glob_pattern: String,
    max_transfer_size: usize,
}

/// Supported mimetypes in order of preference (least to most)
fn supported_mimetypes() -> Vec<SupportedMimetype> {
    vec![
        SupportedMimetype {
            glob_pattern: "image/tiff".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/bmp".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/gif".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/jpeg".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/webp".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/png".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "image/svg+xml".to_string(),
            max_transfer_size: MAX_IMAGE_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "text/plain".to_string(),
            max_transfer_size: MAX_TEXT_SIZE,
        },
        SupportedMimetype {
            glob_pattern: "text/plain;charset=utf-8".to_string(),
            max_transfer_size: MAX_TEXT_SIZE,
        },
    ]
}

/// Clipboard manager state
#[derive(Debug)]
pub struct ClipboardManager {
    pub id: u32,
    /// Saved clipboard content
    pub saved_content: Vec<u8>,
    /// Mimetype of saved content
    pub saved_mimetype: String,
    /// Current selection source owner
    pub current_owner_id: Option<u32>,
}

impl ClipboardManager {
    /// Create new clipboard manager
    pub fn new(id: u32) -> Self {
        ClipboardManager {
            id,
            saved_content: Vec::new(),
            saved_mimetype: String::new(),
            current_owner_id: None,
        }
    }

    /// Check if mimetype matches pattern and get max transfer size
    pub fn match_mimetype(mimetype: &str) -> Option<(usize, usize)> {
        let mimetypes = supported_mimetypes();
        for (idx, mt) in mimetypes.iter().enumerate() {
            // Simple glob matching: exact string match or pattern match
            if simple_glob_match(&mt.glob_pattern, mimetype) {
                return Some((idx, mt.max_transfer_size));
            }
        }
        None
    }

    /// Find best supported mimetype from list (highest priority)
    pub fn select_best_mimetype(mimetypes: &[String]) -> Option<String> {
        let mut best_idx: isize = -1;
        let mut best_mimetype: Option<String> = None;

        for mimetype in mimetypes {
            if let Some((idx, _size)) = Self::match_mimetype(mimetype) {
                if (idx as isize) > best_idx {
                    best_idx = idx as isize;
                    best_mimetype = Some(mimetype.clone());
                }
            }
        }

        best_mimetype
    }

    /// Save clipboard content
    pub fn save_content(&mut self, content: Vec<u8>, mimetype: String) {
        self.saved_content = content;
        self.saved_mimetype = mimetype;
    }

    /// Clear saved clipboard
    pub fn clear_saved(&mut self) {
        self.saved_content.clear();
        self.saved_mimetype.clear();
    }

    /// Handle clipboard owner changed signal
    /// Stub: requires async callback infrastructure for content transfer
    pub fn on_owner_changed(&mut self, _new_owner_id: Option<u32>) {
        // In real GLib implementation:
        // - Cancel pending transfer
        // - If new owner exists: select best mimetype and initiate async transfer
        // - If owner is None but we have saved content: become owner with saved content
        // Stubbed for no_std kernel
    }

    /// Initialize clipboard manager (connect signals)
    /// Stub: requires signal infrastructure
    pub fn init() {
        // Would connect to selection owner-changed signal
    }

    /// Shutdown clipboard manager (disconnect signals)
    /// Stub: requires signal infrastructure
    pub fn shutdown(&mut self) {
        self.clear_saved();
    }
}

/// Simple glob pattern matching (limited subset)
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    // For now, only exact match; full glob would require * and ? support
    pattern == text
}
