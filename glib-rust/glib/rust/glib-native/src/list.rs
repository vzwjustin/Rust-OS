//! Doubly- and singly-linked lists matching `glist.h` / `glist.c` and
//! `gslist.h` / `gslist.c`.
//!
//! [`List`] and [`SList`] own their nodes; element `data` pointers are opaque
//! and their lifetime is the caller's responsibility.

use crate::prelude::*;
use core::ffi::c_void;
use core::mem;
use core::ptr;

/// Comparison function for sort and search (`GCompareFunc`).
///
/// Returns a negative value if `a` sorts before `b`, zero if equal, or a
/// positive value if `a` sorts after `b`.
pub type CompareFn = fn(*const c_void, *const c_void) -> i32;

/// Callback invoked for each element (`GFunc`).
pub type FuncFn = fn(*mut c_void, *mut c_void);

// ---------------------------------------------------------------------------
// GList — doubly-linked list node layout
// ---------------------------------------------------------------------------

/// Doubly-linked list node (`GList`).
#[repr(C)]
#[derive(Debug)]
pub struct GList {
    /// Element payload; not freed when the list is dropped.
    pub data: *mut c_void,
    /// Next node, or null at the tail.
    pub next: *mut GList,
    /// Previous node, or null at the head.
    pub prev: *mut GList,
}

// ---------------------------------------------------------------------------
// GSList — singly-linked list node layout
// ---------------------------------------------------------------------------

/// Singly-linked list node (`GSList`).
#[repr(C)]
#[derive(Debug)]
pub struct GSList {
    /// Element payload; not freed when the list is dropped.
    pub data: *mut c_void,
    /// Next node, or null at the tail.
    pub next: *mut GSList,
}

// ---------------------------------------------------------------------------
// Private GList primitives
// ---------------------------------------------------------------------------

fn g_list_alloc() -> *mut GList {
    Box::into_raw(Box::new(GList {
        data: ptr::null_mut(),
        next: ptr::null_mut(),
        prev: ptr::null_mut(),
    }))
}

fn g_list_free(list: *mut GList) {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            let next = (*current).next;
            drop(Box::from_raw(current));
            current = next;
        }
    }
}

/// # Safety
///
/// `node` must be a node previously detached from any list.
fn g_list_free_1(node: *mut GList) {
    unsafe {
        if !node.is_null() {
            drop(Box::from_raw(node));
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_last(list: *mut GList) -> *mut GList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }
        let mut list = list;
        while !(*list).next.is_null() {
            list = (*list).next;
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_append(list: *mut GList, data: *mut c_void) -> *mut GList {
    unsafe {
        let new_list = g_list_alloc();
        (*new_list).data = data;
        (*new_list).next = ptr::null_mut();

        if list.is_null() {
            (*new_list).prev = ptr::null_mut();
            return new_list;
        }

        let last = g_list_last(list);
        (*last).next = new_list;
        (*new_list).prev = last;
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_prepend(list: *mut GList, data: *mut c_void) -> *mut GList {
    unsafe {
        let new_list = g_list_alloc();
        (*new_list).data = data;
        (*new_list).next = list;

        if !list.is_null() {
            (*new_list).prev = (*list).prev;
            if !(*list).prev.is_null() {
                (*(*list).prev).next = new_list;
            }
            (*list).prev = new_list;
        } else {
            (*new_list).prev = ptr::null_mut();
        }
        new_list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_insert(list: *mut GList, data: *mut c_void, position: i32) -> *mut GList {
    unsafe {
        if position < 0 {
            return g_list_append(list, data);
        }
        if position == 0 {
            return g_list_prepend(list, data);
        }

        let tmp_list = g_list_nth(list, position as u32);
        if tmp_list.is_null() {
            return g_list_append(list, data);
        }

        let new_list = g_list_alloc();
        (*new_list).data = data;
        (*new_list).prev = (*tmp_list).prev;
        (*(*tmp_list).prev).next = new_list;
        (*new_list).next = tmp_list;
        (*tmp_list).prev = new_list;
        list
    }
}

/// # Safety
///
/// `list1` and `list2` must be valid list heads or null.
fn g_list_concat(list1: *mut GList, list2: *mut GList) -> *mut GList {
    unsafe {
        if list2.is_null() {
            return list1;
        }

        if list1.is_null() {
            return list2;
        }

        let last = g_list_last(list1);
        (*last).next = list2;
        (*list2).prev = last;
        list1
    }
}

/// # Safety
///
/// `list` and `link` must be valid; `link` must belong to `list`.
fn g_list_remove_link(mut list: *mut GList, link: *mut GList) -> *mut GList {
    unsafe {
        if link.is_null() {
            return list;
        }

        if !(*link).prev.is_null() && (*(*link).prev).next == link {
            (*(*link).prev).next = (*link).next;
        }
        if !(*link).next.is_null() && (*(*link).next).prev == link {
            (*(*link).next).prev = (*link).prev;
        }

        if link == list {
            list = (*link).next;
        }

        (*link).next = ptr::null_mut();
        (*link).prev = ptr::null_mut();
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_remove(mut list: *mut GList, data: *const c_void) -> *mut GList {
    unsafe {
        let mut tmp = list;
        while !tmp.is_null() {
            if core::ptr::eq((*tmp).data as *const c_void, data) {
                let removed = tmp;
                list = g_list_remove_link(list, removed);
                g_list_free_1(removed);
                break;
            }
            tmp = (*tmp).next;
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_remove_all(mut list: *mut GList, data: *const c_void) -> *mut GList {
    unsafe {
        let mut tmp = list;
        while !tmp.is_null() {
            if core::ptr::eq((*tmp).data as *const c_void, data) {
                let next = (*tmp).next;
                list = g_list_remove_link(list, tmp);
                g_list_free_1(tmp);
                tmp = next;
            } else {
                tmp = (*tmp).next;
            }
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_copy(list: *mut GList) -> *mut GList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }

        let new_list = g_list_alloc();
        (*new_list).data = (*list).data;
        (*new_list).prev = ptr::null_mut();
        let mut last = new_list;
        let mut src = (*list).next;

        while !src.is_null() {
            let node = g_list_alloc();
            (*node).data = (*src).data;
            (*node).prev = last;
            (*last).next = node;
            last = node;
            src = (*src).next;
        }
        (*last).next = ptr::null_mut();
        new_list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_reverse(list: *mut GList) -> *mut GList {
    unsafe {
        let mut last = ptr::null_mut();
        let mut current = list;

        while !current.is_null() {
            last = current;
            let next = (*current).next;
            mem::swap(&mut (*current).next, &mut (*current).prev);
            current = next;
        }
        last
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_nth(list: *mut GList, n: u32) -> *mut GList {
    unsafe {
        let mut list = list;
        let mut n = n;
        while n > 0 && !list.is_null() {
            list = (*list).next;
            n -= 1;
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_nth_data(list: *mut GList, n: u32) -> *mut c_void {
    unsafe {
        let node = g_list_nth(list, n);
        if node.is_null() {
            ptr::null_mut()
        } else {
            (*node).data
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_length(list: *mut GList) -> u32 {
    unsafe {
        let mut len = 0;
        let mut current = list;
        while !current.is_null() {
            len += 1;
            current = (*current).next;
        }
        len
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_find(list: *mut GList, data: *const c_void) -> *mut GList {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            if core::ptr::eq((*current).data as *const c_void, data) {
                return current;
            }
            current = (*current).next;
        }
        ptr::null_mut()
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_find_custom(list: *mut GList, data: *const c_void, func: CompareFn) -> *mut GList {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            if func((*current).data, data) == 0 {
                return current;
            }
            current = (*current).next;
        }
        ptr::null_mut()
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_foreach(list: *mut GList, func: FuncFn, user_data: *mut c_void) {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            let next = (*current).next;
            func((*current).data, user_data);
            current = next;
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_list_sort(list: *mut GList, compare_func: CompareFn) -> *mut GList {
    g_list_sort_real(list, compare_func)
}

fn g_list_sort_real(list: *mut GList, compare_func: CompareFn) -> *mut GList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }
        if (*list).next.is_null() {
            return list;
        }

        let mut l1 = list;
        let mut l2 = (*list).next;

        while !l2.is_null() {
            l2 = (*l2).next;
            if l2.is_null() {
                break;
            }
            l2 = (*l2).next;
            if l2.is_null() {
                break;
            }
            l1 = (*l1).next;
        }

        l2 = (*l1).next;
        (*l1).next = ptr::null_mut();

        g_list_sort_merge(
            g_list_sort_real(list, compare_func),
            g_list_sort_real(l2, compare_func),
            compare_func,
        )
    }
}

fn g_list_sort_merge(l1: *mut GList, l2: *mut GList, compare_func: CompareFn) -> *mut GList {
    unsafe {
        let head = GList {
            data: ptr::null_mut(),
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        };
        let sentinel = Box::into_raw(Box::new(head));
        let mut tail = sentinel;

        let mut l1 = l1;
        let mut l2 = l2;

        while !l1.is_null() && !l2.is_null() {
            let cmp = compare_func((*l1).data, (*l2).data);
            let chosen = if cmp <= 0 { &mut l1 } else { &mut l2 };

            (*tail).next = *chosen;
            (*(*chosen)).prev = tail;
            tail = *chosen;
            *chosen = (*(*chosen)).next;
        }

        (*tail).next = if !l1.is_null() { l1 } else { l2 };
        if !(*tail).next.is_null() {
            (*(*tail).next).prev = tail;
        }

        let result = (*sentinel).next;
        drop(Box::from_raw(sentinel));
        result
    }
}

// ---------------------------------------------------------------------------
// Private GSList primitives
// ---------------------------------------------------------------------------

fn g_slist_alloc() -> *mut GSList {
    Box::into_raw(Box::new(GSList {
        data: ptr::null_mut(),
        next: ptr::null_mut(),
    }))
}

fn g_slist_free(list: *mut GSList) {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            let next = (*current).next;
            drop(Box::from_raw(current));
            current = next;
        }
    }
}

/// # Safety
///
/// `node` must be a node previously detached from any list.
fn g_slist_free_1(node: *mut GSList) {
    unsafe {
        if !node.is_null() {
            drop(Box::from_raw(node));
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_last(list: *mut GSList) -> *mut GSList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }
        let mut list = list;
        while !(*list).next.is_null() {
            list = (*list).next;
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_append(list: *mut GSList, data: *mut c_void) -> *mut GSList {
    unsafe {
        let new_list = g_slist_alloc();
        (*new_list).data = data;
        (*new_list).next = ptr::null_mut();

        if list.is_null() {
            return new_list;
        }

        let last = g_slist_last(list);
        (*last).next = new_list;
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_prepend(list: *mut GSList, data: *mut c_void) -> *mut GSList {
    unsafe {
        let new_list = g_slist_alloc();
        (*new_list).data = data;
        (*new_list).next = list;
        new_list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_insert(list: *mut GSList, data: *mut c_void, position: i32) -> *mut GSList {
    unsafe {
        if position < 0 {
            return g_slist_append(list, data);
        }
        if position == 0 {
            return g_slist_prepend(list, data);
        }

        let new_list = g_slist_alloc();
        (*new_list).data = data;

        if list.is_null() {
            (*new_list).next = ptr::null_mut();
            return new_list;
        }

        let mut prev_list = ptr::null_mut();
        let mut tmp_list = list;
        let mut position = position;

        while position > 0 && !tmp_list.is_null() {
            prev_list = tmp_list;
            tmp_list = (*tmp_list).next;
            position -= 1;
        }

        (*new_list).next = (*prev_list).next;
        (*prev_list).next = new_list;
        list
    }
}

/// # Safety
///
/// `list1` and `list2` must be valid list heads or null.
fn g_slist_concat(list1: *mut GSList, list2: *mut GSList) -> *mut GSList {
    unsafe {
        if list2.is_null() {
            return list1;
        }
        if list1.is_null() {
            return list2;
        }
        let last = g_slist_last(list1);
        (*last).next = list2;
        list1
    }
}

fn g_slist_remove_data(list: *mut GSList, data: *const c_void, all: bool) -> *mut GSList {
    unsafe {
        let mut list = list;
        let mut previous_ptr: *mut *mut GSList = &mut list;

        while !(*previous_ptr).is_null() {
            let tmp = *previous_ptr;
            if core::ptr::eq((*tmp).data as *const c_void, data) {
                *previous_ptr = (*tmp).next;
                g_slist_free_1(tmp);
                if !all {
                    break;
                }
            } else {
                previous_ptr = &mut (*tmp).next;
            }
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_remove(list: *mut GSList, data: *const c_void) -> *mut GSList {
    g_slist_remove_data(list, data, false)
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_remove_all(list: *mut GSList, data: *const c_void) -> *mut GSList {
    g_slist_remove_data(list, data, true)
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_copy(list: *mut GSList) -> *mut GSList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }

        let new_list = g_slist_alloc();
        (*new_list).data = (*list).data;
        let mut last = new_list;
        let mut src = (*list).next;

        while !src.is_null() {
            let node = g_slist_alloc();
            (*node).data = (*src).data;
            (*last).next = node;
            last = node;
            src = (*src).next;
        }
        (*last).next = ptr::null_mut();
        new_list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_reverse(list: *mut GSList) -> *mut GSList {
    unsafe {
        let mut prev = ptr::null_mut();
        let mut current = list;

        while !current.is_null() {
            let next = (*current).next;
            (*current).next = prev;
            prev = current;
            current = next;
        }
        prev
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_nth(list: *mut GSList, n: u32) -> *mut GSList {
    unsafe {
        let mut list = list;
        let mut n = n;
        while n > 0 && !list.is_null() {
            list = (*list).next;
            n -= 1;
        }
        list
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_nth_data(list: *mut GSList, n: u32) -> *mut c_void {
    unsafe {
        let node = g_slist_nth(list, n);
        if node.is_null() {
            ptr::null_mut()
        } else {
            (*node).data
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_length(list: *mut GSList) -> u32 {
    unsafe {
        let mut len = 0;
        let mut current = list;
        while !current.is_null() {
            len += 1;
            current = (*current).next;
        }
        len
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_find(list: *mut GSList, data: *const c_void) -> *mut GSList {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            if core::ptr::eq((*current).data as *const c_void, data) {
                return current;
            }
            current = (*current).next;
        }
        ptr::null_mut()
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_find_custom(list: *mut GSList, data: *const c_void, func: CompareFn) -> *mut GSList {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            if func((*current).data, data) == 0 {
                return current;
            }
            current = (*current).next;
        }
        ptr::null_mut()
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_foreach(list: *mut GSList, func: FuncFn, user_data: *mut c_void) {
    unsafe {
        let mut current = list;
        while !current.is_null() {
            let next = (*current).next;
            func((*current).data, user_data);
            current = next;
        }
    }
}

/// # Safety
///
/// `list` must be a valid list head or null.
fn g_slist_sort(list: *mut GSList, compare_func: CompareFn) -> *mut GSList {
    g_slist_sort_real(list, compare_func)
}

fn g_slist_sort_real(list: *mut GSList, compare_func: CompareFn) -> *mut GSList {
    unsafe {
        if list.is_null() {
            return ptr::null_mut();
        }
        if (*list).next.is_null() {
            return list;
        }

        let mut l1 = list;
        let mut l2 = (*list).next;

        while !l2.is_null() {
            l2 = (*l2).next;
            if l2.is_null() {
                break;
            }
            l2 = (*l2).next;
            if l2.is_null() {
                break;
            }
            l1 = (*l1).next;
        }

        l2 = (*l1).next;
        (*l1).next = ptr::null_mut();

        g_slist_sort_merge(
            g_slist_sort_real(list, compare_func),
            g_slist_sort_real(l2, compare_func),
            compare_func,
        )
    }
}

fn g_slist_sort_merge(l1: *mut GSList, l2: *mut GSList, compare_func: CompareFn) -> *mut GSList {
    unsafe {
        let head = GSList {
            data: ptr::null_mut(),
            next: ptr::null_mut(),
        };
        let sentinel = Box::into_raw(Box::new(head));
        let mut tail = sentinel;

        let mut l1 = l1;
        let mut l2 = l2;

        while !l1.is_null() && !l2.is_null() {
            let cmp = compare_func((*l1).data, (*l2).data);
            if cmp <= 0 {
                (*tail).next = l1;
                tail = l1;
                l1 = (*l1).next;
            } else {
                (*tail).next = l2;
                tail = l2;
                l2 = (*l2).next;
            }
        }

        (*tail).next = if !l1.is_null() { l1 } else { l2 };

        let result = (*sentinel).next;
        drop(Box::from_raw(sentinel));
        result
    }
}

// ---------------------------------------------------------------------------
// List — owned doubly-linked list
// ---------------------------------------------------------------------------

/// Owned doubly-linked list (`GList`).
pub struct List {
    head: *mut GList,
}

impl Default for List {
    fn default() -> Self {
        Self {
            head: ptr::null_mut(),
        }
    }
}

impl Drop for List {
    fn drop(&mut self) {
        g_list_free(self.head);
    }
}

impl List {
    /// Empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Head pointer for FFI (`g_list` ownership remains with this value).
    pub fn as_ptr(&self) -> *mut GList {
        self.head
    }

    /// Take ownership of an existing list head.
    ///
    /// # Safety
    ///
    /// `ptr` must be a list allocated by this module or null.
    pub unsafe fn from_ptr(ptr: *mut GList) -> Self {
        Self { head: ptr }
    }

    /// Release ownership without freeing nodes.
    pub fn into_ptr(self) -> *mut GList {
        let ptr = self.head;
        mem::forget(self);
        ptr
    }

    /// Whether the list has no nodes.
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Append `data` at the tail (`g_list_append`).
    pub fn append(&mut self, data: *mut c_void) {
        self.head = g_list_append(self.head, data);
    }

    /// Prepend `data` at the head (`g_list_prepend`).
    pub fn prepend(&mut self, data: *mut c_void) {
        self.head = g_list_prepend(self.head, data);
    }

    /// Insert `data` at `position` (`g_list_insert`).
    pub fn insert(&mut self, data: *mut c_void, position: i32) {
        self.head = g_list_insert(self.head, data, position);
    }

    /// Remove the first node whose `data` equals `data` (`g_list_remove`).
    pub fn remove(&mut self, data: *const c_void) {
        self.head = g_list_remove(self.head, data);
    }

    /// Remove every node whose `data` equals `data` (`g_list_remove_all`).
    pub fn remove_all(&mut self, data: *const c_void) {
        self.head = g_list_remove_all(self.head, data);
    }

    /// Number of nodes (`g_list_length`).
    pub fn length(&self) -> u32 {
        g_list_length(self.head)
    }

    /// Node at zero-based index `n`, or null (`g_list_nth`).
    pub fn nth(&self, n: u32) -> *mut GList {
        g_list_nth(self.head, n)
    }

    /// `data` at zero-based index `n` (`g_list_nth_data`).
    pub fn nth_data(&self, n: u32) -> *mut c_void {
        g_list_nth_data(self.head, n)
    }

    /// First node whose `data` pointer equals `data` (`g_list_find`).
    pub fn find(&self, data: *const c_void) -> *mut GList {
        g_list_find(self.head, data)
    }

    /// Find a node using `func` (`g_list_find_custom`).
    pub fn find_custom(&self, data: *const c_void, func: CompareFn) -> *mut GList {
        g_list_find_custom(self.head, data, func)
    }

    /// Invoke `func` for each element's data (`g_list_foreach`).
    pub fn foreach(&self, func: FuncFn, user_data: *mut c_void) {
        g_list_foreach(self.head, func, user_data);
    }

    /// Shallow copy sharing `data` pointers (`g_list_copy`).
    pub fn copy(&self) -> Self {
        Self {
            head: g_list_copy(self.head),
        }
    }

    /// Append `other` at the tail; `other` is consumed (`g_list_concat`).
    pub fn concat(&mut self, mut other: Self) {
        self.head = g_list_concat(self.head, other.head);
        other.head = ptr::null_mut();
    }

    /// Reverse the list in place (`g_list_reverse`).
    pub fn reverse(&mut self) {
        self.head = g_list_reverse(self.head);
    }

    /// Stable sort (`g_list_sort`).
    pub fn sort(&mut self, compare_func: CompareFn) {
        self.head = g_list_sort(self.head, compare_func);
    }
}

// ---------------------------------------------------------------------------
// SList — owned singly-linked list
// ---------------------------------------------------------------------------

/// Owned singly-linked list (`GSList`).
pub struct SList {
    head: *mut GSList,
}

impl Default for SList {
    fn default() -> Self {
        Self {
            head: ptr::null_mut(),
        }
    }
}

impl Drop for SList {
    fn drop(&mut self) {
        g_slist_free(self.head);
    }
}

impl SList {
    /// Empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Head pointer for FFI (`g_slist` ownership remains with this value).
    pub fn as_ptr(&self) -> *mut GSList {
        self.head
    }

    /// Take ownership of an existing list head.
    ///
    /// # Safety
    ///
    /// `ptr` must be a list allocated by this module or null.
    pub unsafe fn from_ptr(ptr: *mut GSList) -> Self {
        Self { head: ptr }
    }

    /// Release ownership without freeing nodes.
    pub fn into_ptr(self) -> *mut GSList {
        let ptr = self.head;
        mem::forget(self);
        ptr
    }

    /// Whether the list has no nodes.
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Append `data` at the tail (`g_slist_append`).
    pub fn append(&mut self, data: *mut c_void) {
        self.head = g_slist_append(self.head, data);
    }

    /// Prepend `data` at the head (`g_slist_prepend`).
    pub fn prepend(&mut self, data: *mut c_void) {
        self.head = g_slist_prepend(self.head, data);
    }

    /// Insert `data` at `position` (`g_slist_insert`).
    pub fn insert(&mut self, data: *mut c_void, position: i32) {
        self.head = g_slist_insert(self.head, data, position);
    }

    /// Remove the first node whose `data` equals `data` (`g_slist_remove`).
    pub fn remove(&mut self, data: *const c_void) {
        self.head = g_slist_remove(self.head, data);
    }

    /// Remove every node whose `data` equals `data` (`g_slist_remove_all`).
    pub fn remove_all(&mut self, data: *const c_void) {
        self.head = g_slist_remove_all(self.head, data);
    }

    /// Number of nodes (`g_slist_length`).
    pub fn length(&self) -> u32 {
        g_slist_length(self.head)
    }

    /// Node at zero-based index `n`, or null (`g_slist_nth`).
    pub fn nth(&self, n: u32) -> *mut GSList {
        g_slist_nth(self.head, n)
    }

    /// `data` at zero-based index `n` (`g_slist_nth_data`).
    pub fn nth_data(&self, n: u32) -> *mut c_void {
        g_slist_nth_data(self.head, n)
    }

    /// First node whose `data` pointer equals `data` (`g_slist_find`).
    pub fn find(&self, data: *const c_void) -> *mut GSList {
        g_slist_find(self.head, data)
    }

    /// Find a node using `func` (`g_slist_find_custom`).
    pub fn find_custom(&self, data: *const c_void, func: CompareFn) -> *mut GSList {
        g_slist_find_custom(self.head, data, func)
    }

    /// Invoke `func` for each element's data (`g_slist_foreach`).
    pub fn foreach(&self, func: FuncFn, user_data: *mut c_void) {
        g_slist_foreach(self.head, func, user_data);
    }

    /// Shallow copy sharing `data` pointers (`g_slist_copy`).
    pub fn copy(&self) -> Self {
        Self {
            head: g_slist_copy(self.head),
        }
    }

    /// Append `other` at the tail; `other` is consumed (`g_slist_concat`).
    pub fn concat(&mut self, mut other: Self) {
        self.head = g_slist_concat(self.head, other.head);
        other.head = ptr::null_mut();
    }

    /// Reverse the list in place (`g_slist_reverse`).
    pub fn reverse(&mut self) {
        self.head = g_slist_reverse(self.head);
    }

    /// Stable sort (`g_slist_sort`).
    pub fn sort(&mut self, compare_func: CompareFn) {
        self.head = g_slist_sort(self.head, compare_func);
    }
}

// ---------------------------------------------------------------------------
// Tests (mirrors `glib/tests/list.c` subsets)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn int_to_ptr(n: i32) -> *mut c_void {
        (n as isize) as *mut c_void
    }

    fn ptr_to_int(p: *const c_void) -> i32 {
        p as isize as i32
    }

    fn int_compare(a: *const c_void, b: *const c_void) -> i32 {
        let a = ptr_to_int(a);
        let b = ptr_to_int(b);
        a.cmp(&b) as i32
    }

    fn int_compare_eq(a: *const c_void, b: *const c_void) -> i32 {
        if ptr_to_int(a) == ptr_to_int(b) {
            0
        } else {
            1
        }
    }

    #[test]
    fn glist_append_and_length() {
        let mut list = List::new();
        list.append(int_to_ptr(1));
        list.append(int_to_ptr(2));
        list.append(int_to_ptr(3));

        assert_eq!(list.length(), 3);
        assert!(!list.is_empty());
        assert_eq!(ptr_to_int(list.nth_data(0)), 1);
        assert_eq!(ptr_to_int(list.nth_data(2)), 3);
    }

    #[test]
    fn glist_prepend_order() {
        let mut list = List::new();
        list.prepend(int_to_ptr(2));
        list.prepend(int_to_ptr(1));

        assert_eq!(ptr_to_int(list.nth_data(0)), 1);
        assert_eq!(ptr_to_int(list.nth_data(1)), 2);
    }

    #[test]
    fn glist_remove_first_match() {
        let mut list = List::new();
        for i in 0..10 {
            list.append(int_to_ptr(i));
            list.append(int_to_ptr(i));
        }
        assert_eq!(list.length(), 20);

        for i in 0..10 {
            list.remove(int_to_ptr(i));
        }

        assert_eq!(list.length(), 10);
        for i in 0..10 {
            assert_eq!(ptr_to_int(list.nth_data(i as u32)), i);
        }
    }

    #[test]
    fn glist_remove_all() {
        let mut list = List::new();
        for i in 0..10 {
            list.append(int_to_ptr(i));
            list.append(int_to_ptr(i));
        }
        assert_eq!(list.length(), 20);

        for i in 0..5 {
            list.remove_all(int_to_ptr(2 * i + 1));
            list.remove_all(int_to_ptr(8 - 2 * i));
        }

        assert!(list.is_empty());
        assert_eq!(list.length(), 0);
    }

    #[test]
    fn glist_find_and_nth() {
        let mut list = List::new();
        let nums: [i32; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        for &n in &nums {
            list.append(int_to_ptr(n));
        }

        for (i, &n) in nums.iter().enumerate() {
            let node = list.nth(i as u32);
            assert!(!node.is_null());
            assert_eq!(ptr_to_int(unsafe { (*node).data }), n);
        }

        let found = list.find(int_to_ptr(5));
        assert!(!found.is_null());
        assert_eq!(ptr_to_int(unsafe { (*found).data }), 5);

        assert!(list.find(int_to_ptr(99)).is_null());
    }

    #[test]
    fn glist_find_custom() {
        let mut list = List::new();
        list.append(int_to_ptr(10));
        list.append(int_to_ptr(20));

        let found = list.find_custom(int_to_ptr(20), int_compare_eq);
        assert!(!found.is_null());
        assert_eq!(ptr_to_int(unsafe { (*found).data }), 20);
    }

    #[test]
    fn glist_concat() {
        let mut list1 = List::new();
        let mut list2 = List::new();
        for i in 0..5 {
            list1.append(int_to_ptr(i));
            list2.append(int_to_ptr(i + 5));
        }

        list1.concat(list2);
        assert_eq!(list1.length(), 10);
        for i in 0..10 {
            assert_eq!(ptr_to_int(list1.nth_data(i)), i as i32);
        }
    }

    #[test]
    fn glist_reverse() {
        let mut list = List::new();
        for i in 0..10 {
            list.append(int_to_ptr(i));
        }

        list.reverse();
        for i in 0..10 {
            assert_eq!(ptr_to_int(list.nth_data(i)), (9 - i) as i32);
        }
    }

    #[test]
    fn glist_copy_shares_data() {
        let mut list = List::new();
        list.append(int_to_ptr(1));
        list.append(int_to_ptr(2));

        let copy = list.copy();
        let mut u = list.as_ptr();
        let mut v = copy.as_ptr();
        while !u.is_null() && !v.is_null() {
            unsafe {
                assert_eq!((*u).data, (*v).data);
                u = (*u).next;
                v = (*v).next;
            }
        }
        assert!(u.is_null() && v.is_null());
    }

    #[test]
    fn glist_sort() {
        let values = [5, 1, 4, 2, 8, 0, 2];
        let mut list = List::new();
        for &v in &values {
            list.append(int_to_ptr(v));
        }

        list.sort(int_compare);
        for i in 0..list.length() - 1 {
            let a = ptr_to_int(list.nth_data(i));
            let b = ptr_to_int(list.nth_data(i + 1));
            assert!(a <= b);
        }
    }

    #[test]
    fn glist_insert() {
        let mut list = List::new();
        list.insert(int_to_ptr(1), 0);
        list.insert(int_to_ptr(3), 1);
        list.insert(int_to_ptr(2), 1);

        assert_eq!(ptr_to_int(list.nth_data(0)), 1);
        assert_eq!(ptr_to_int(list.nth_data(1)), 2);
        assert_eq!(ptr_to_int(list.nth_data(2)), 3);
    }

    #[test]
    fn glist_foreach() {
        static SEEN: AtomicU32 = AtomicU32::new(0);

        fn accumulate(data: *mut c_void, _user: *mut c_void) {
            SEEN.fetch_add(ptr_to_int(data) as u32, Ordering::SeqCst);
        }

        SEEN.store(0, Ordering::SeqCst);
        let mut list = List::new();
        list.append(int_to_ptr(1));
        list.append(int_to_ptr(2));
        list.append(int_to_ptr(3));

        list.foreach(accumulate, ptr::null_mut());
        assert_eq!(SEEN.load(Ordering::SeqCst), 6);
    }

    #[test]
    fn gslist_append_prepend_find() {
        let mut list = SList::new();
        list.append(int_to_ptr(2));
        list.prepend(int_to_ptr(1));

        assert_eq!(list.length(), 2);
        assert_eq!(ptr_to_int(list.nth_data(0)), 1);
        assert_eq!(ptr_to_int(list.nth_data(1)), 2);

        let found = list.find(int_to_ptr(2));
        assert!(!found.is_null());
    }

    #[test]
    fn gslist_remove_and_remove_all() {
        let mut list = SList::new();
        list.append(int_to_ptr(1));
        list.append(int_to_ptr(2));
        list.append(int_to_ptr(1));

        list.remove(int_to_ptr(1));
        assert_eq!(list.length(), 2);

        list.remove_all(int_to_ptr(2));
        assert_eq!(list.length(), 1);
        assert_eq!(ptr_to_int(list.nth_data(0)), 1);
    }

    #[test]
    fn gslist_sort_and_reverse() {
        let mut list = SList::new();
        for v in [3, 1, 4, 1, 5] {
            list.append(int_to_ptr(v));
        }

        list.sort(int_compare);
        assert_eq!(ptr_to_int(list.nth_data(0)), 1);

        list.reverse();
        assert_eq!(ptr_to_int(list.nth_data(0)), 5);
    }
}
