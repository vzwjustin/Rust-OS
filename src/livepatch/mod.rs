//! Live kernel patching — symbol redirect registration table.
//!
//! Patches register old→new symbol mappings; [`resolve_symbol`] returns the
//! replacement when a patch is active.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// One symbol redirect entry.
#[derive(Debug, Clone, Copy)]
pub struct SymbolRedirect {
    pub old_addr: usize,
    pub new_addr: usize,
}

/// Registered livepatch object.
#[derive(Debug, Clone)]
pub struct Livepatch {
    pub name: String,
    pub version: String,
    pub redirects: BTreeMap<String, SymbolRedirect>,
    pub active: bool,
}

static PATCHES: RwLock<BTreeMap<String, Livepatch>> = RwLock::new(BTreeMap::new());

/// Register a livepatch (inactive until [`apply_patch`]).
pub fn register_patch(name: &str, version: &str) -> bool {
    let mut patches = PATCHES.write();
    if patches.contains_key(name) {
        return false;
    }
    patches.insert(
        String::from(name),
        Livepatch {
            name: String::from(name),
            version: String::from(version),
            redirects: BTreeMap::new(),
            active: false,
        },
    );
    true
}

/// Add a symbol redirect to a registered patch.
pub fn add_redirect(patch_name: &str, symbol: &str, old_addr: usize, new_addr: usize) -> bool {
    let mut patches = PATCHES.write();
    let Some(patch) = patches.get_mut(patch_name) else {
        return false;
    };
    patch
        .redirects
        .insert(String::from(symbol), SymbolRedirect { old_addr, new_addr });
    true
}

/// Activate a registered patch.
pub fn apply_patch(name: &str) -> bool {
    let mut patches = PATCHES.write();
    let Some(patch) = patches.get_mut(name) else {
        return false;
    };
    if patch.active {
        return true;
    }
    patch.active = true;
    crate::serial_println!(
        "[livepatch] applied '{}' v{} ({} redirects)",
        patch.name,
        patch.version,
        patch.redirects.len()
    );
    true
}

/// Deactivate a patch.
pub fn revert_patch(name: &str) -> bool {
    let mut patches = PATCHES.write();
    let Some(patch) = patches.get_mut(name) else {
        return false;
    };
    patch.active = false;
    true
}

/// Resolve `symbol` to replacement address from active patches.
pub fn resolve_symbol(symbol: &str) -> Option<usize> {
    let patches = PATCHES.read();
    for patch in patches.values() {
        if patch.active {
            if let Some(r) = patch.redirects.get(symbol) {
                return Some(r.new_addr);
            }
        }
    }
    None
}

/// Resolve by original address (for indirect callsites).
pub fn resolve_address(old_addr: usize) -> usize {
    let patches = PATCHES.read();
    for patch in patches.values() {
        if !patch.active {
            continue;
        }
        for r in patch.redirects.values() {
            if r.old_addr == old_addr {
                return r.new_addr;
            }
        }
    }
    old_addr
}

/// List registered patches.
pub fn list_patches() -> Vec<(String, bool, usize)> {
    PATCHES
        .read()
        .values()
        .map(|p| (p.name.clone(), p.active, p.redirects.len()))
        .collect()
}

/// Initialize livepatch registry.
pub fn init() {
    PATCHES.write().clear();
    crate::serial_println!("[livepatch] symbol redirect table initialized");
}
