//! Pure, `no_std` application-grid model: the catalog of launchable apps plus
//! search-filter and pagination helpers. Deliberately free of `WindowManager`
//! state so the logic stays unit-testable on its own. The renderer and input
//! handling live in `window_manager.rs`.

use heapless::Vec;

/// Apps shown per page. Small on purpose: the built-in catalog spans more than
/// one page so pagination is actually exercised.
pub const ITEMS_PER_PAGE: usize = 4;

/// Upper bound on the catalog / a filtered result set.
pub const MAX_APPS: usize = 32;

/// A launchable application. `slot` indexes into `WindowManager::launch_app_slot`,
/// reusing the dock's existing launch wiring instead of a parallel code path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppEntry {
    pub name: &'static str,
    pub icon: char,
    pub slot: usize,
}

/// Built-in application catalog. Slots match `launch_app_slot`; slot 3 is the
/// Activities toggle, so it is intentionally absent.
pub const CATALOG: &[AppEntry] = &[
    AppEntry {
        name: "Terminal",
        icon: 'T',
        slot: 0,
    },
    AppEntry {
        name: "Files",
        icon: 'F',
        slot: 1,
    },
    AppEntry {
        name: "System Monitor",
        icon: 'M',
        slot: 2,
    },
    AppEntry {
        name: "Text Editor",
        icon: 'E',
        slot: 4,
    },
    AppEntry {
        name: "Network",
        icon: 'N',
        slot: 5,
    },
];

/// Case-insensitive ASCII substring test.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    'outer: for start in 0..=h.len() - n.len() {
        for i in 0..n.len() {
            if !h[start + i].eq_ignore_ascii_case(&n[i]) {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

/// Apps whose name matches `query` (case-insensitive substring; empty = all).
pub fn filter(query: &str) -> Vec<&'static AppEntry, MAX_APPS> {
    let mut out = Vec::new();
    for app in CATALOG {
        if contains_ci(app.name, query) {
            let _ = out.push(app);
        }
    }
    out
}

/// Number of pages needed for `count` items (always at least 1).
pub fn page_count(count: usize) -> usize {
    if count == 0 {
        1
    } else {
        (count + ITEMS_PER_PAGE - 1) / ITEMS_PER_PAGE
    }
}

/// The slice of `items` shown on `page` (returns empty for an out-of-range page).
pub fn page_slice<'a>(items: &'a [&'static AppEntry], page: usize) -> &'a [&'static AppEntry] {
    let start = page * ITEMS_PER_PAGE;
    if start >= items.len() {
        return &[];
    }
    let end = (start + ITEMS_PER_PAGE).min(items.len());
    &items[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn filter_empty_returns_all() {
        assert_eq!(filter("").len(), CATALOG.len());
    }

    #[test_case]
    fn filter_is_case_insensitive() {
        let r = filter("fIlEs");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "Files");
    }

    #[test_case]
    fn filter_substring_matches_multiple() {
        // "te" matches "Terminal", "sysTEm Monitor", and "Text Editor".
        assert_eq!(filter("te").len(), 3);
    }

    #[test_case]
    fn filter_no_match_is_empty() {
        assert!(filter("zzz").is_empty());
    }

    #[test_case]
    fn page_count_basics() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(4), 1);
        assert_eq!(page_count(5), 2);
    }

    #[test_case]
    fn page_slice_ranges() {
        let all = filter(""); // 5 apps, 4 per page
        assert_eq!(page_slice(&all, 0).len(), 4);
        assert_eq!(page_slice(&all, 1).len(), 1);
        assert!(page_slice(&all, 2).is_empty());
    }
}
