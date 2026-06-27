//! Sorting matching `gqsort.h` / `gqsort.c`.
//!
//! Provides `qsort_with_data` and `sort_array` as wrappers around
//! Rust's built-in slice sorting. Fully `no_std` compatible.

/// Sort a slice with a comparison function (`g_qsort_with_data` / `g_sort_array`).
///
/// Uses Rust's `sort_by` which is a stable sort (mergesort).
pub fn sort_array<T, F>(slice: &mut [T], compare: F)
where
    F: FnMut(&T, &T) -> core::cmp::Ordering,
{
    slice.sort_by(compare);
}

/// Sort a slice with a comparison function (unstable, for performance).
///
/// Uses Rust's `sort_unstable_by` which is typically introsort.
/// Prefer this when stability is not required.
pub fn sort_array_unstable<T, F>(slice: &mut [T], compare: F)
where
    F: FnMut(&T, &T) -> core::cmp::Ordering,
{
    slice.sort_unstable_by(compare);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    #[test]
    fn sort_ints() {
        let mut v = [3, 1, 4, 1, 5, 9, 2, 6];
        sort_array(&mut v, |a, b| a.cmp(b));
        assert_eq!(v, [1, 1, 2, 3, 4, 5, 6, 9]);
    }

    #[test]
    fn sort_desc() {
        let mut v = [3, 1, 4, 1, 5];
        sort_array(&mut v, |a, b| b.cmp(a));
        assert_eq!(v, [5, 4, 3, 1, 1]);
    }

    #[test]
    fn sort_empty() {
        let mut v: [i32; 0] = [];
        sort_array(&mut v, |a, b| a.cmp(b));
        assert_eq!(v, []);
    }

    #[test]
    fn sort_single() {
        let mut v = [42];
        sort_array(&mut v, |a, b| a.cmp(b));
        assert_eq!(v, [42]);
    }

    #[test]
    fn sort_unstable() {
        let mut v = [5, 2, 8, 1, 9, 3];
        sort_array_unstable(&mut v, |a, b| a.cmp(b));
        assert_eq!(v, [1, 2, 3, 5, 8, 9]);
    }

    #[test]
    fn sort_strings() {
        let mut v = ["banana", "apple", "cherry"];
        sort_array(&mut v, |a, b| a.cmp(b));
        assert_eq!(v, ["apple", "banana", "cherry"]);
    }
}
