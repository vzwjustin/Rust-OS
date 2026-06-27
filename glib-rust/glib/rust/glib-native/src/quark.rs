//! Quark interning matching `gquark.h` / `gquark.c`.
//!
//! A [`Quark`] is a non-zero integer that uniquely identifies an interned string.
//! Quark `0` corresponds to `None` / `NULL`. The global table is shared by the
//! quark and string-interning APIs.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use spin::rwlock::RwLock;
use spin::Once;

/// Non-zero identifier for an interned string (`GQuark`).
pub type Quark = u32;

const QUARK_BLOCK_SIZE: usize = 2048;
const QUARK_STRING_BLOCK_SIZE: usize = 4096 - core::mem::size_of::<usize>();

struct QuarkGlobal {
    ht: BTreeMap<&'static str, Quark>,
    quarks: Vec<Option<&'static str>>,
    next_id: Quark,
    string_block: Option<Vec<u8>>,
    string_block_offset: usize,
}

impl QuarkGlobal {
    fn new() -> Self {
        Self {
            ht: BTreeMap::new(),
            quarks: vec![None; QUARK_BLOCK_SIZE],
            next_id: 1,
            string_block: None,
            string_block_offset: 0,
        }
    }
}

fn global() -> &'static RwLock<QuarkGlobal> {
    static GLOBAL: Once<RwLock<QuarkGlobal>> = Once::new();
    GLOBAL.call_once(|| RwLock::new(QuarkGlobal::new()))
}

/// Looks up a quark for `string` without creating one (`g_quark_try_string`).
///
/// Returns `0` when `string` is `None` or not yet interned.
pub fn quark_try_string(string: Option<&str>) -> Quark {
    let Some(string) = string else {
        return 0;
    };

    global()
        .read()
        .ht
        .get(string)
        .copied()
        .unwrap_or(0)
}

/// Returns the quark for `string`, interning a copy when needed (`g_quark_from_string`).
///
/// Returns `0` when `string` is `None`.
pub fn quark_from_string(string: Option<&str>) -> Quark {
    let Some(string) = string else {
        return 0;
    };

    let state = global().read();
    if let Some(&quark) = state.ht.get(string) {
        return quark;
    }
    drop(state);

    let mut state = global().write();
    if let Some(&quark) = state.ht.get(string) {
        return quark;
    }

    let stored = quark_strdup(string, &mut state);
    quark_new(stored, &mut state)
}

/// Returns the quark for a process-lifetime `string` without copying it
/// (`g_quark_from_static_string`).
///
/// Returns `0` when `string` is `None`.
pub fn quark_from_static_string(string: Option<&'static str>) -> Quark {
    let Some(string) = string else {
        return 0;
    };

    let state = global().read();
    if let Some(&quark) = state.ht.get(string) {
        return quark;
    }
    drop(state);

    let mut state = global().write();
    if let Some(&quark) = state.ht.get(string) {
        return quark;
    }

    quark_new(string, &mut state)
}

/// Returns the interned string for `quark` (`g_quark_to_string`).
///
/// Returns `None` for quark `0` or out-of-range values.
pub fn quark_to_string(quark: Quark) -> Option<&'static str> {
    if quark == 0 {
        return None;
    }

    let state = global().read();
    if (quark as usize) < state.next_id as usize {
        state.quarks.get(quark as usize).copied().flatten()
    } else {
        None
    }
}

/// Returns the canonical interned representation of `string` (`g_intern_string`).
pub fn intern_string(string: Option<&str>) -> Option<&'static str> {
    let string = string?;
    let quark = quark_from_string(Some(string));
    quark_to_string(quark)
}

/// Returns the canonical interned representation of a static `string`
/// (`g_intern_static_string`).
pub fn intern_static_string(string: Option<&'static str>) -> Option<&'static str> {
    let string = string?;
    let quark = quark_from_static_string(Some(string));
    quark_to_string(quark)
}

/// Copies `string` into the quark string pool (`quark_strdup`).
///
/// Callers must hold the global write lock.
fn quark_strdup(string: &str, state: &mut QuarkGlobal) -> &'static str {
    let len = string.len();

    if len > QUARK_STRING_BLOCK_SIZE / 2 {
        return Box::leak(string.to_owned().into_boxed_str());
    }

    if state.string_block.is_none() || QUARK_STRING_BLOCK_SIZE - state.string_block_offset < len {
        if let Some(old) = state.string_block.take() {
            let _ = Box::leak(old.into_boxed_slice());
        }
        state.string_block = Some(vec![0u8; QUARK_STRING_BLOCK_SIZE]);
        state.string_block_offset = 0;
    }

    let block = state.string_block.as_mut().unwrap();
    let start = state.string_block_offset;
    let end = start + len;
    block[start..end].copy_from_slice(string.as_bytes());
    state.string_block_offset = end;

    // SAFETY: `string` was valid UTF-8 and we copied it verbatim. Block memory is
    // never freed until the block is rotated, at which point it is leaked.
    unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(block.as_ptr().add(start), len))
    }
}

/// Inserts a new quark for `string` (`quark_new`).
///
/// Callers must hold the global write lock.
fn quark_new(string: &'static str, state: &mut QuarkGlobal) -> Quark {
    if (state.next_id as usize).is_multiple_of(QUARK_BLOCK_SIZE) {
        state
            .quarks
            .resize(state.next_id as usize + QUARK_BLOCK_SIZE, None);
    }

    let quark = state.next_id;
    state.quarks[quark as usize] = Some(string);
    state.ht.insert(string, quark);
    state.next_id += 1;
    quark
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn quark_try_string_null_and_missing() {
        assert_eq!(quark_try_string(None), 0);
        assert_eq!(quark_try_string(Some("not-interned-yet-try")), 0);
    }

    #[test]
    fn quark_from_string_interns_and_reuses() {
        let q1 = quark_from_string(Some("hello-quark"));
        let q2 = quark_from_string(Some("hello-quark"));
        assert_ne!(q1, 0);
        assert_eq!(q1, q2);
        assert_eq!(quark_to_string(q1), Some("hello-quark"));
    }

    #[test]
    fn quark_from_string_null_returns_zero() {
        assert_eq!(quark_from_string(None), 0);
    }

    #[test]
    fn quark_from_static_string_reuses_without_copy() {
        static TEXT: &str = "static-quark-text";
        let q1 = quark_from_static_string(Some(TEXT));
        let q2 = quark_from_static_string(Some(TEXT));
        assert_ne!(q1, 0);
        assert_eq!(q1, q2);
        assert_eq!(quark_to_string(q1), Some(TEXT));
    }

    #[test]
    fn quark_to_string_invalid_and_zero() {
        assert_eq!(quark_to_string(0), None);
        assert_eq!(quark_to_string(Quark::MAX), None);
    }

    #[test]
    fn quark_try_string_finds_existing() {
        let q = quark_from_string(Some("try-existing"));
        assert_eq!(quark_try_string(Some("try-existing")), q);
    }

    #[test]
    fn intern_string_returns_canonical_pointer() {
        let a = intern_string(Some("intern-me")).unwrap();
        let b = intern_string(Some("intern-me")).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, "intern-me");
    }

    #[test]
    fn intern_static_string_returns_input_for_static() {
        static TEXT: &str = "intern-static";
        let a = intern_static_string(Some(TEXT)).unwrap();
        let b = intern_static_string(Some(TEXT)).unwrap();
        assert_eq!(a, TEXT);
        assert_eq!(a, b);
    }

    #[test]
    fn distinct_strings_get_distinct_quarks() {
        let q1 = quark_from_string(Some("alpha-distinct"));
        let q2 = quark_from_string(Some("beta-distinct"));
        assert_ne!(q1, q2);
        assert_ne!(quark_to_string(q1), quark_to_string(q2));
    }

    #[test]
    fn intern_string_null_returns_none() {
        assert_eq!(intern_string(None), None);
        assert_eq!(intern_static_string(None), None);
    }

    #[test]
    fn concurrent_intern_is_stable() {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let label = format!("concurrent-quark-{id}");

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let label = label.clone();
                std::thread::spawn(move || quark_from_string(Some(&label)))
            })
            .collect();

        let quarks: Vec<Quark> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(quarks.iter().all(|q| *q == quarks[0] && *q != 0));
        assert_eq!(quark_to_string(quarks[0]), Some(label.as_str()));
    }
}
