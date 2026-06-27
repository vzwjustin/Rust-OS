//! GIO file attribute info list matching `gio/gfileattribute.h` /
//! `gio/gfileattribute.c`.
//!
//! First GIO submodule ported (Phase 11 entry point). Provides:
//! - `FileAttributeType` enum (the 10 attribute data types).
//! - `FileAttributeInfoFlags` flags (copy-with-file, copy-when-moved).
//! - `FileAttributeInfo` struct (name + type + flags).
//! - `FileAttributeInfoList` — ref-counted, sorted-by-name list with
//!   binary-search lookup and insert, matching upstream
//!   `GFileAttributeInfoList`.
//!
//! Fully `no_std` compatible using `alloc` and `spin` (for the atomic
//! ref count, though `Arc` would also work — we use `Arc` directly).

use crate::prelude::*;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

// ─────────────────────── GFileAttributeType ───────────────────────────────

/// File attribute data type (`GFileAttributeType`).
///
/// Matches the upstream enum order so the discriminant values are stable
/// across the C and Rust implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum FileAttributeType {
    /// Invalid or uninitialized (`G_FILE_ATTRIBUTE_TYPE_INVALID`).
    #[default]
    Invalid = 0,
    /// NUL-terminated UTF-8 string (`G_FILE_ATTRIBUTE_TYPE_STRING`).
    String = 1,
    /// NUL-terminated byte string (`G_FILE_ATTRIBUTE_TYPE_BYTE_STRING`).
    ByteString = 2,
    /// Boolean (`G_FILE_ATTRIBUTE_TYPE_BOOLEAN`).
    Boolean = 3,
    /// Unsigned 32-bit integer (`G_FILE_ATTRIBUTE_TYPE_UINT32`).
    Uint32 = 4,
    /// Signed 32-bit integer (`G_FILE_ATTRIBUTE_TYPE_INT32`).
    Int32 = 5,
    /// Unsigned 64-bit integer (`G_FILE_ATTRIBUTE_TYPE_UINT64`).
    Uint64 = 6,
    /// Signed 64-bit integer (`G_FILE_ATTRIBUTE_TYPE_INT64`).
    Int64 = 7,
    /// GObject (`G_FILE_ATTRIBUTE_TYPE_OBJECT`).
    Object = 8,
    /// NUL-terminated string vector (`G_FILE_ATTRIBUTE_TYPE_STRINGV`).
    Stringv = 9,
}

impl FileAttributeType {
    /// Human-readable name matching upstream `g_file_attribute_type_to_string`.
    pub fn as_str(self) -> &'static str {
        match self {
            FileAttributeType::Invalid => "invalid",
            FileAttributeType::String => "string",
            FileAttributeType::ByteString => "bytestring",
            FileAttributeType::Boolean => "boolean",
            FileAttributeType::Uint32 => "uint32",
            FileAttributeType::Int32 => "int32",
            FileAttributeType::Uint64 => "uint64",
            FileAttributeType::Int64 => "int64",
            FileAttributeType::Object => "object",
            FileAttributeType::Stringv => "stringv",
        }
    }
}

// ───────────────────── GFileAttributeInfoFlags ────────────────────────────

/// Flags specifying the behaviour of a file attribute
/// (`GFileAttributeInfoFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FileAttributeInfoFlags(pub u32);

impl FileAttributeInfoFlags {
    /// No flags set (`G_FILE_ATTRIBUTE_INFO_NONE`).
    pub const NONE: Self = Self(0);
    /// Copy the attribute values when the file is copied
    /// (`G_FILE_ATTRIBUTE_INFO_COPY_WITH_FILE`).
    pub const COPY_WITH_FILE: Self = Self(1 << 0);
    /// Copy the attribute values when the file is moved
    /// (`G_FILE_ATTRIBUTE_INFO_COPY_WHEN_MOVED`).
    pub const COPY_WHEN_MOVED: Self = Self(1 << 1);

    /// Returns `true` if `other` is set in `self`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for FileAttributeInfoFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// ─────────────────────── GFileAttributeInfo ───────────────────────────────

/// Information about a specific file attribute (`GFileAttributeInfo`).
///
/// Upstream is `{ char *name; GFileAttributeType type; GFileAttributeInfoFlags flags; }`.
/// We own the name as a `String` so no manual freeing is needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileAttributeInfo {
    /// The name of the attribute, e.g. `"standard::name"`.
    pub name: String,
    /// The data type of the attribute.
    pub r#type: FileAttributeType,
    /// Behaviour flags (copy-with-file, copy-when-moved).
    pub flags: FileAttributeInfoFlags,
}

// ─────────────────── GFileAttributeInfoList ───────────────────────────────

/// Inner state of a `FileAttributeInfoList`, shared via `Arc` for ref
/// counting. The list is kept sorted by `name` so `lookup` and `add`
/// can use binary search, matching upstream's
/// `g_file_attribute_info_list_bsearch`.
#[derive(Debug)]
struct FileAttributeInfoListInner {
    infos: Vec<FileAttributeInfo>,
}

impl FileAttributeInfoListInner {
    fn new() -> Self {
        Self { infos: Vec::new() }
    }

    /// Binary search for `name` in the sorted infos vector. Returns the
    /// index where `name` is, or the index where it would be inserted to
    /// keep the vector sorted. Mirrors `g_file_attribute_info_list_bsearch`.
    fn bsearch(&self, name: &str) -> usize {
        let mut start = 0usize;
        let mut end = self.infos.len();
        while start != end {
            let mid = start + (end - start) / 2;
            match name.cmp(&self.infos[mid].name) {
                core::cmp::Ordering::Less => end = mid,
                core::cmp::Ordering::Greater => start = mid + 1,
                core::cmp::Ordering::Equal => return mid,
            }
        }
        start
    }
}

/// A ref-counted, sorted-by-name list of `FileAttributeInfo`
/// (`GFileAttributeInfoList`).
///
/// Upstream uses an atomic ref count + a `GArray` of `GFileAttributeInfo`.
/// We use `Arc<Inner>` so clone bumps the ref count and dropping
/// decrements it. The public API mirrors upstream:
/// - `new` / `dup` / `ref_` / `unref`
/// - `lookup` — binary search by name
/// - `add` — insert (or update if name exists) keeping the list sorted
/// - `n_infos` / `infos` / `info` — accessors
#[derive(Clone, Debug)]
pub struct FileAttributeInfoList {
    inner: Arc<FileAttributeInfoListInner>,
}

impl FileAttributeInfoList {
    /// Create a new empty list (`g_file_attribute_info_list_new`).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(FileAttributeInfoListInner::new()),
        }
    }

    /// Duplicate the list (`g_file_attribute_info_list_dup`).
    ///
    /// Returns a deep copy — modifications to the duplicate do not affect
    /// the original. Upstream returns a new ref-counted struct; we return
    /// a new `Arc` so the two are independent.
    pub fn dup(&self) -> Self {
        let mut new_inner = FileAttributeInfoListInner::new();
        for info in self.inner.infos.iter() {
            new_inner.infos.push(info.clone());
        }
        Self {
            inner: Arc::new(new_inner),
        }
    }

    /// Reference the list (`g_file_attribute_info_list_ref`).
    ///
    /// With `Arc` this is just `clone`. Kept for API parity; upstream
    /// returns the same pointer, we return a new `Arc` handle to the
    /// same shared state.
    pub fn ref_(&self) -> Self {
        self.clone()
    }

    /// Look up an attribute by name (`g_file_attribute_info_list_lookup`).
    ///
    /// Returns `Some(&info)` if found, `None` otherwise.
    pub fn lookup(&self, name: &str) -> Option<&FileAttributeInfo> {
        let i = self.inner.bsearch(name);
        if i < self.inner.infos.len() && self.inner.infos[i].name == name {
            Some(&self.inner.infos[i])
        } else {
            None
        }
    }

    /// Add an attribute to the list (`g_file_attribute_info_list_add`).
    ///
    /// If `name` already exists, its type is updated (matching upstream
    /// behaviour). Otherwise a new entry is inserted at the position
    /// that keeps the list sorted by name.
    ///
    /// **Note**: this method requires `Arc::get_mut` to succeed, i.e. the
    /// list must be uniquely owned (ref count == 1). If the list is
    /// shared, call `dup` first and mutate the clone.
    pub fn add(
        &mut self,
        name: &str,
        attr_type: FileAttributeType,
        flags: FileAttributeInfoFlags,
    ) -> Result<(), &'static str> {
        // Coerce uniqueness so we don't mutate state visible through
        // other Arc handles. If shared, the caller should dup first.
        let inner = Arc::get_mut(&mut self.inner).ok_or("list is shared; dup first")?;
        let i = inner.bsearch(name);
        if i < inner.infos.len() && inner.infos[i].name == name {
            inner.infos[i].r#type = attr_type;
            inner.infos[i].flags = flags;
        } else {
            inner.infos.insert(
                i,
                FileAttributeInfo {
                    name: name.to_owned(),
                    r#type: attr_type,
                    flags,
                },
            );
        }
        Ok(())
    }

    /// Number of attributes in the list (`GFileAttributeInfoList.n_infos`).
    pub fn n_infos(&self) -> usize {
        self.inner.infos.len()
    }

    /// Borrow the i-th attribute. Panics if `i >= n_infos()`, matching
    /// upstream array indexing.
    pub fn info(&self, i: usize) -> &FileAttributeInfo {
        &self.inner.infos[i]
    }

    /// Borrow the entire infos slice.
    pub fn infos(&self) -> &[FileAttributeInfo] {
        &self.inner.infos
    }

    /// Current ref count (for diagnostics / smoke checks).
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl Default for FileAttributeInfoList {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_type_values_match_upstream() {
        assert_eq!(FileAttributeType::Invalid as u32, 0);
        assert_eq!(FileAttributeType::String as u32, 1);
        assert_eq!(FileAttributeType::ByteString as u32, 2);
        assert_eq!(FileAttributeType::Boolean as u32, 3);
        assert_eq!(FileAttributeType::Uint32 as u32, 4);
        assert_eq!(FileAttributeType::Int32 as u32, 5);
        assert_eq!(FileAttributeType::Uint64 as u32, 6);
        assert_eq!(FileAttributeType::Int64 as u32, 7);
        assert_eq!(FileAttributeType::Object as u32, 8);
        assert_eq!(FileAttributeType::Stringv as u32, 9);
    }

    #[test]
    fn attribute_type_to_string() {
        assert_eq!(FileAttributeType::String.as_str(), "string");
        assert_eq!(FileAttributeType::ByteString.as_str(), "bytestring");
        assert_eq!(FileAttributeType::Invalid.as_str(), "invalid");
        assert_eq!(FileAttributeType::Stringv.as_str(), "stringv");
    }

    #[test]
    fn info_flags_bitor_and_contains() {
        let flags =
            FileAttributeInfoFlags::COPY_WITH_FILE | FileAttributeInfoFlags::COPY_WHEN_MOVED;
        assert!(flags.contains(FileAttributeInfoFlags::COPY_WITH_FILE));
        assert!(flags.contains(FileAttributeInfoFlags::COPY_WHEN_MOVED));
        // NONE is 0 so contains(NONE) is trivially true for any flags;
        // verify the bit values instead.
        assert_eq!(FileAttributeInfoFlags::NONE.0, 0);
        assert_eq!(FileAttributeInfoFlags::COPY_WITH_FILE.0, 1);
        assert_eq!(FileAttributeInfoFlags::COPY_WHEN_MOVED.0, 2);
        assert_eq!((flags.0 & 0x03), 0x03);
    }

    #[test]
    fn new_list_is_empty() {
        let list = FileAttributeInfoList::new();
        assert_eq!(list.n_infos(), 0);
        assert_eq!(list.infos().len(), 0);
        assert_eq!(list.ref_count(), 1);
    }

    #[test]
    fn add_and_lookup() {
        let mut list = FileAttributeInfoList::new();
        list.add(
            "standard::name",
            FileAttributeType::String,
            FileAttributeInfoFlags::COPY_WITH_FILE,
        )
        .unwrap();
        list.add(
            "standard::size",
            FileAttributeType::Uint64,
            FileAttributeInfoFlags::NONE,
        )
        .unwrap();
        assert_eq!(list.n_infos(), 2);

        let info = list.lookup("standard::name").unwrap();
        assert_eq!(info.name, "standard::name");
        assert_eq!(info.r#type, FileAttributeType::String);
        assert_eq!(info.flags, FileAttributeInfoFlags::COPY_WITH_FILE);

        let info2 = list.lookup("standard::size").unwrap();
        assert_eq!(info2.r#type, FileAttributeType::Uint64);
        assert_eq!(info2.flags, FileAttributeInfoFlags::NONE);

        assert!(list.lookup("standard::nonexistent").is_none());
    }

    #[test]
    fn add_updates_existing_name() {
        let mut list = FileAttributeInfoList::new();
        list.add(
            "standard::type",
            FileAttributeType::String,
            FileAttributeInfoFlags::NONE,
        )
        .unwrap();
        // Re-adding the same name updates the type, doesn't add a new entry.
        list.add(
            "standard::type",
            FileAttributeType::Int32,
            FileAttributeInfoFlags::COPY_WHEN_MOVED,
        )
        .unwrap();
        assert_eq!(list.n_infos(), 1);
        let info = list.lookup("standard::type").unwrap();
        assert_eq!(info.r#type, FileAttributeType::Int32);
        assert_eq!(info.flags, FileAttributeInfoFlags::COPY_WHEN_MOVED);
    }

    #[test]
    fn add_keeps_list_sorted() {
        let mut list = FileAttributeInfoList::new();
        // Insert in non-sorted order; the list should stay sorted by name.
        list.add("c", FileAttributeType::String, FileAttributeInfoFlags::NONE)
            .unwrap();
        list.add("a", FileAttributeType::String, FileAttributeInfoFlags::NONE)
            .unwrap();
        list.add("b", FileAttributeType::String, FileAttributeInfoFlags::NONE)
            .unwrap();
        assert_eq!(list.n_infos(), 3);
        assert_eq!(list.infos()[0].name, "a");
        assert_eq!(list.infos()[1].name, "b");
        assert_eq!(list.infos()[2].name, "c");
    }

    #[test]
    fn dup_is_independent() {
        let mut list = FileAttributeInfoList::new();
        list.add(
            "standard::name",
            FileAttributeType::String,
            FileAttributeInfoFlags::NONE,
        )
        .unwrap();
        let mut copy = list.dup();
        // Mutating the copy shouldn't affect the original.
        copy.add(
            "standard::size",
            FileAttributeType::Uint64,
            FileAttributeInfoFlags::NONE,
        )
        .unwrap();
        assert_eq!(copy.n_infos(), 2);
        assert_eq!(list.n_infos(), 1);
        // Original should NOT have "standard::size".
        assert!(list.lookup("standard::size").is_none());
    }

    #[test]
    fn ref_count_increments_with_clone() {
        let list = FileAttributeInfoList::new();
        assert_eq!(list.ref_count(), 1);
        let list2 = list.ref_();
        assert_eq!(list.ref_count(), 2);
        assert_eq!(list2.ref_count(), 2);
        drop(list2);
        assert_eq!(list.ref_count(), 1);
    }

    #[test]
    fn add_on_shared_list_errors() {
        let mut list = FileAttributeInfoList::new();
        let _shared = list.clone(); // bump ref count to 2
        let res = list.add("x", FileAttributeType::String, FileAttributeInfoFlags::NONE);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("shared"));
    }

    #[test]
    fn info_indexing() {
        let mut list = FileAttributeInfoList::new();
        list.add("a", FileAttributeType::String, FileAttributeInfoFlags::NONE)
            .unwrap();
        list.add(
            "b",
            FileAttributeType::Boolean,
            FileAttributeInfoFlags::NONE,
        )
        .unwrap();
        assert_eq!(list.info(0).name, "a");
        assert_eq!(list.info(1).name, "b");
        assert_eq!(list.info(1).r#type, FileAttributeType::Boolean);
    }

    #[test]
    #[should_panic]
    fn info_out_of_bounds_panics() {
        let list = FileAttributeInfoList::new();
        let _ = list.info(0);
    }

    #[test]
    fn lookup_on_empty_returns_none() {
        let list = FileAttributeInfoList::new();
        assert!(list.lookup("anything").is_none());
    }

    #[test]
    fn binary_search_correctness_with_many_entries() {
        let mut list = FileAttributeInfoList::new();
        // Insert 26 entries in reverse order; lookup must find each.
        for c in "zyxwvutsrqponmlkjihgfedcba".chars() {
            let name: String = c.to_string();
            list.add(
                &name,
                FileAttributeType::String,
                FileAttributeInfoFlags::NONE,
            )
            .unwrap();
        }
        assert_eq!(list.n_infos(), 26);
        // Verify sorted.
        for i in 0..25 {
            assert!(list.infos()[i].name < list.infos()[i + 1].name);
        }
        // Every letter should be findable.
        for c in "abcdefghijklmnopqrstuvwxyz".chars() {
            let name: String = c.to_string();
            assert!(list.lookup(&name).is_some(), "missing {c}");
        }
    }
}
