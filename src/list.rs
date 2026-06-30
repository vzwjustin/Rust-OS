// SPDX-License-Identifier: GPL-2.0
//! Intrusive doubly-linked list — pure Rust port of Linux kernel list.rs
//!
//! Unlike `std::collections::LinkedList`, this list is *intrusive*: the
//! prev/next link fields live inside the element itself via `ListLinks`.
//! This means no separate node allocation is needed; the item IS the node.
//!
//! # Safety model
//!
//! All mutation is done through raw pointers and is marked `unsafe`.  The
//! public API is safe by construction (invariants are upheld by the types).

#![allow(dead_code, unused_variables, unused_imports, clippy::mut_from_ref)]

extern crate alloc;
use alloc::boxed::Box;
use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ptr::NonNull,
};

// ---------------------------------------------------------------------------
// ListLinks — embedded prev/next pointers
// ---------------------------------------------------------------------------

/// Embedded prev/next pointers for an intrusive list node.
///
/// A type that wants to live in an intrusive list must contain a field of
/// type `ListLinks` (or `ListLinks<ID>` for multi-list membership).
///
/// # Invariants
///
/// When the node is in a list, `prev` and `next` point to valid nodes in
/// that list (which may be the sentinel head).  When not in a list both
/// pointers are null.
pub struct ListLinks<const ID: u64 = 0> {
    // Both are null when the node is not in a list.
    prev: *mut ListLinks<ID>,
    next: *mut ListLinks<ID>,
}

// SAFETY: the raw pointers are only accessed while holding the owning
// list (which provides mutual exclusion).
unsafe impl<const ID: u64> Send for ListLinks<ID> {}
unsafe impl<const ID: u64> Sync for ListLinks<ID> {}

impl<const ID: u64> ListLinks<ID> {
    /// Creates a new, detached (null) `ListLinks`.
    pub const fn new() -> Self {
        Self {
            prev: core::ptr::null_mut(),
            next: core::ptr::null_mut(),
        }
    }

    /// Returns `true` if this node is not currently in any list.
    pub fn is_detached(&self) -> bool {
        self.next.is_null()
    }
}

impl<const ID: u64> Default for ListLinks<ID> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HasListLinks — trait implemented by list-able types
// ---------------------------------------------------------------------------

/// Implemented by types that contain a `ListLinks<ID>` field.
///
/// # Safety
///
/// `links_ptr` must return a pointer to the `ListLinks<ID>` field within
/// `Self`, and the pointer must remain valid for the lifetime of `Self`.
pub unsafe trait HasListLinks<const ID: u64 = 0> {
    /// Returns a raw pointer to the embedded `ListLinks<ID>` field.
    ///
    /// # Safety
    ///
    /// `self_ptr` must point to a valid, non-moved `Self`.
    unsafe fn links_ptr(self_ptr: *mut Self) -> *mut ListLinks<ID>;
}

// ---------------------------------------------------------------------------
// ListArc — ownership token for items in a List
// ---------------------------------------------------------------------------

/// Ownership token for a heap-allocated item that may be inserted into a
/// [`List`].  Wraps a `Box<T>`; exactly one `ListArc` can exist per item.
///
/// When `ListArc` is dropped without being inserted into a list the
/// underlying `Box` is freed normally.
pub struct ListArc<T> {
    inner: Box<T>,
}

impl<T> ListArc<T> {
    /// Creates a new `ListArc` owning `item`.
    pub fn new(item: T) -> Self {
        Self { inner: Box::new(item) }
    }

    /// Returns a raw pointer to the contained item (not consumed).
    pub fn as_ptr(&self) -> *mut T {
        &*self.inner as *const T as *mut T
    }

    /// Consumes the `ListArc` and returns the raw pointer, transferring
    /// ownership.  The caller is responsible for eventually calling
    /// `ListArc::from_raw` or freeing via `Box::from_raw`.
    pub fn into_raw(self) -> *mut T {
        Box::into_raw(self.inner)
    }

    /// Reconstructs a `ListArc` from a raw pointer previously obtained via
    /// `into_raw`.
    ///
    /// # Safety
    ///
    /// `ptr` must have been produced by `ListArc::into_raw` and must not
    /// already be in a `List`.
    pub unsafe fn from_raw(ptr: *mut T) -> Self {
        Self { inner: unsafe { Box::from_raw(ptr) } }
    }

    /// Returns a shared reference to the contained item.
    pub fn as_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the contained item.
    pub fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> core::ops::Deref for ListArc<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.inner }
}

impl<T> core::ops::DerefMut for ListArc<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.inner }
}

// ---------------------------------------------------------------------------
// List<T, ID> — the list head
// ---------------------------------------------------------------------------

/// An intrusive doubly-linked list of `T` items.
///
/// The list uses a sentinel (dummy) head node whose `next` field points to
/// the first element and whose `prev` field points to the last element.
/// An empty list has `head.next == head.prev == &head`.
///
/// `T` must implement [`HasListLinks<ID>`].
pub struct List<T: HasListLinks<ID>, const ID: u64 = 0> {
    /// Sentinel head node.  Its `prev`/`next` are pointers into the
    /// **sentinel itself** when the list is empty, and into real `T` items
    /// otherwise.
    head: ListLinks<ID>,
    len: usize,
    _phantom: PhantomData<T>,
}

// SAFETY: if `T: Send`, a `List<T>` may cross thread boundaries.
unsafe impl<T: HasListLinks<ID> + Send, const ID: u64> Send for List<T, ID> {}

impl<T: HasListLinks<ID>, const ID: u64> Default for List<T, ID> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: HasListLinks<ID>, const ID: u64> List<T, ID> {
    /// Creates an empty list.
    pub fn new() -> Self {
        let mut list = Self {
            head: ListLinks::new(),
            len: 0,
            _phantom: PhantomData,
        };
        // Make the sentinel point at itself.
        let p = &mut list.head as *mut ListLinks<ID>;
        list.head.prev = p;
        list.head.next = p;
        list
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the number of items currently in the list.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns a raw pointer to the sentinel head.
    fn head_ptr(&self) -> *mut ListLinks<ID> {
        &self.head as *const ListLinks<ID> as *mut ListLinks<ID>
    }

    /// Inserts `item` before the `next` node (internal helper).
    ///
    /// # Safety
    ///
    /// `item_links` must not already be in any list.
    /// `before` must be a valid list node.
    unsafe fn insert_before(
        item_links: *mut ListLinks<ID>,
        before: *mut ListLinks<ID>,
    ) {
        unsafe {
            let prev = (*before).prev;
            (*item_links).next = before;
            (*item_links).prev = prev;
            (*prev).next = item_links;
            (*before).prev = item_links;
        }
    }

    /// Unlinks a node from the list (internal helper).
    ///
    /// # Safety
    ///
    /// `links` must be in this list.
    unsafe fn unlink(links: *mut ListLinks<ID>) {
        unsafe {
            let prev = (*links).prev;
            let next = (*links).next;
            (*prev).next = next;
            (*next).prev = prev;
            (*links).prev = core::ptr::null_mut();
            (*links).next = core::ptr::null_mut();
        }
    }

    // ------------------------------------------------------------------
    // Public mutation API
    // ------------------------------------------------------------------

    /// Appends `item` to the back of the list.
    ///
    /// Ownership of the item is transferred to the list.
    pub fn push_back(&mut self, item: ListArc<T>) {
        let raw: *mut T = item.into_raw();
        let links = unsafe { T::links_ptr(raw) };
        // Insert before the sentinel head = at the back.
        unsafe { Self::insert_before(links, self.head_ptr()) };
        self.len += 1;
    }

    /// Prepends `item` to the front of the list.
    pub fn push_front(&mut self, item: ListArc<T>) {
        let raw: *mut T = item.into_raw();
        let links = unsafe { T::links_ptr(raw) };
        // Insert before the first real node (= `head.next`).
        let first = unsafe { (*self.head_ptr()).next };
        unsafe { Self::insert_before(links, first) };
        self.len += 1;
    }

    /// Removes and returns the first item, or `None` if empty.
    pub fn pop_front(&mut self) -> Option<ListArc<T>> {
        if self.is_empty() { return None; }
        let links = unsafe { (*self.head_ptr()).next };
        unsafe { Self::unlink(links) };
        self.len -= 1;
        // Recover *mut T from *mut ListLinks.  We need the offset back.
        // We rely on the impl of HasListLinks to tell us the offset.
        // A simpler trick: we store a *mut T alongside links via a wrapper,
        // but since we can't here, we recover via parent pointer arithmetic.
        // The item was allocated as a Box<T>; HasListLinks gives us:
        //   links_ptr(item_ptr) == links  =>  item_ptr = links - offset
        // Use the offset-recovery helper below.
        let item_ptr = unsafe { Self::item_from_links(links) };
        Some(unsafe { ListArc::from_raw(item_ptr) })
    }

    /// Removes and returns the last item, or `None` if empty.
    pub fn pop_back(&mut self) -> Option<ListArc<T>> {
        if self.is_empty() { return None; }
        let links = unsafe { (*self.head_ptr()).prev };
        unsafe { Self::unlink(links) };
        self.len -= 1;
        let item_ptr = unsafe { Self::item_from_links(links) };
        Some(unsafe { ListArc::from_raw(item_ptr) })
    }

    /// Moves all items from `other` to the back of `self`, leaving `other` empty.
    pub fn push_all_back(&mut self, other: &mut Self) {
        if other.is_empty() { return; }
        let other_first = unsafe { (*other.head_ptr()).next };
        let other_last  = unsafe { (*other.head_ptr()).prev };
        let self_last   = unsafe { (*self.head_ptr()).prev };
        let self_head   = self.head_ptr();

        unsafe {
            (*self_last).next  = other_first;
            (*other_first).prev = self_last;
            (*other_last).next  = self_head;
            (*self_head).prev   = other_last;
        }
        self.len += other.len;
        // Reset other to empty
        let oh = other.head_ptr();
        unsafe {
            (*oh).next = oh;
            (*oh).prev = oh;
        }
        other.len = 0;
    }

    /// Recovers the `*mut T` from a `*mut ListLinks<ID>` by scanning
    /// backwards by the field offset.
    ///
    /// This works because `HasListLinks::links_ptr(item_ptr)` is a deterministic
    /// offset computation.  We find the offset by calling `links_ptr` on a
    /// zeroed-out slot.
    ///
    /// # Safety
    ///
    /// `links` must point to the `ListLinks<ID>` field of a live `T`.
    unsafe fn item_from_links(links: *mut ListLinks<ID>) -> *mut T {
        // Compute the byte offset of the ListLinks field within T.
        // We use a stack-allocated dummy to avoid UB from null-ptr arithmetic.
        let mut dummy = core::mem::MaybeUninit::<T>::uninit();
        let dummy_ptr = dummy.as_mut_ptr();
        let links_in_dummy = unsafe { T::links_ptr(dummy_ptr) } as usize;
        let offset = links_in_dummy - dummy_ptr as usize;
        (links as usize - offset) as *mut T
    }

    // ------------------------------------------------------------------
    // Iterators
    // ------------------------------------------------------------------

    /// Returns a forward iterator over shared references.
    pub fn iter(&self) -> Iter<'_, T, ID> {
        let head = self.head_ptr();
        let cur = unsafe { (*head).next };
        Iter { cur, head, _phantom: PhantomData }
    }

    /// Returns a forward iterator over mutable references.
    pub fn iter_mut(&mut self) -> IterMut<'_, T, ID> {
        let head = self.head_ptr();
        let cur = unsafe { (*head).next };
        IterMut { cur, head, _phantom: PhantomData }
    }
}

impl<T: HasListLinks<ID>, const ID: u64> Drop for List<T, ID> {
    fn drop(&mut self) {
        // Free all remaining items.
        while self.pop_front().is_some() {}
    }
}

// ---------------------------------------------------------------------------
// Iter
// ---------------------------------------------------------------------------

/// Forward iterator over shared references in a [`List`].
pub struct Iter<'a, T: HasListLinks<ID>, const ID: u64 = 0> {
    cur: *mut ListLinks<ID>,
    head: *mut ListLinks<ID>,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T: HasListLinks<ID>, const ID: u64> Iterator for Iter<'a, T, ID> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.cur == self.head {
            return None;
        }
        let item_ptr = unsafe { List::<T, ID>::item_from_links(self.cur) };
        self.cur = unsafe { (*self.cur).next };
        Some(unsafe { &*item_ptr })
    }
}

/// Forward iterator over mutable references in a [`List`].
pub struct IterMut<'a, T: HasListLinks<ID>, const ID: u64 = 0> {
    cur: *mut ListLinks<ID>,
    head: *mut ListLinks<ID>,
    _phantom: PhantomData<&'a mut T>,
}

impl<'a, T: HasListLinks<ID>, const ID: u64> Iterator for IterMut<'a, T, ID> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.cur == self.head {
            return None;
        }
        let item_ptr = unsafe { List::<T, ID>::item_from_links(self.cur) };
        self.cur = unsafe { (*self.cur).next };
        Some(unsafe { &mut *item_ptr })
    }
}

// ---------------------------------------------------------------------------
// impl IntoIterator for &List / &mut List
// ---------------------------------------------------------------------------

impl<'a, T: HasListLinks<ID>, const ID: u64> IntoIterator for &'a List<T, ID> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T, ID>;
    fn into_iter(self) -> Iter<'a, T, ID> {
        self.iter()
    }
}

impl<'a, T: HasListLinks<ID>, const ID: u64> IntoIterator for &'a mut List<T, ID> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T, ID>;
    fn into_iter(self) -> IterMut<'a, T, ID> {
        self.iter_mut()
    }
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

/// A cursor pointing at a position in a [`List`], allowing insertion and
/// removal at that position.
pub struct Cursor<'a, T: HasListLinks<ID>, const ID: u64 = 0> {
    list: &'a mut List<T, ID>,
    cur: *mut ListLinks<ID>,
}

impl<'a, T: HasListLinks<ID>, const ID: u64> Cursor<'a, T, ID> {
    /// Returns a shared reference to the element at the cursor, or `None`
    /// if the cursor is at the sentinel (past-the-end / before-the-start).
    pub fn current(&self) -> Option<&T> {
        if self.cur == self.list.head_ptr() {
            return None;
        }
        Some(unsafe { &*List::<T, ID>::item_from_links(self.cur) })
    }

    /// Advances the cursor forward.
    pub fn move_next(&mut self) {
        self.cur = unsafe { (*self.cur).next };
    }

    /// Moves the cursor backward.
    pub fn move_prev(&mut self) {
        self.cur = unsafe { (*self.cur).prev };
    }

    /// Inserts `item` before the current position.
    pub fn insert_before(&mut self, item: ListArc<T>) {
        let raw = item.into_raw();
        let links = unsafe { T::links_ptr(raw) };
        unsafe { List::<T, ID>::insert_before(links, self.cur) };
        self.list.len += 1;
    }

    /// Removes and returns the item at the current cursor position.
    /// After removal the cursor moves to the next element.
    pub fn remove_current(&mut self) -> Option<ListArc<T>> {
        if self.cur == self.list.head_ptr() {
            return None;
        }
        let links = self.cur;
        let next = unsafe { (*links).next };
        unsafe { List::<T, ID>::unlink(links) };
        self.list.len -= 1;
        self.cur = next;
        let item_ptr = unsafe { List::<T, ID>::item_from_links(links) };
        Some(unsafe { ListArc::from_raw(item_ptr) })
    }
}

impl<'a, T: HasListLinks<ID>, const ID: u64> List<T, ID> {
    /// Returns a [`Cursor`] positioned at the front of the list.
    pub fn cursor_front(&mut self) -> Cursor<'_, T, ID> {
        let cur = unsafe { (*self.head_ptr()).next };
        Cursor { list: self, cur }
    }

    /// Returns a [`Cursor`] positioned past the end (sentinel).
    pub fn cursor_back(&mut self) -> Cursor<'_, T, ID> {
        let cur = self.head_ptr();
        Cursor { list: self, cur }
    }
}
