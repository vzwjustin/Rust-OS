//! Resizable pointer array matching `GPtrArray` from `garray.h` / `garray.c`.

use crate::prelude::*;
use crate::refcount::AtomicRefCount;
use core::cell::RefCell;
use core::cmp::Ordering;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;

const MIN_ARRAY_CAPACITY: u32 = 16;

/// Opaque pointer element (`gpointer`).
pub type GPointer = *mut ();

/// Element destructor callback storage.
type ElementFreeFn = Option<Box<dyn FnMut(GPointer)>>;

/// Comparison function for [`PtrArray::sort`]: receives pointers to array slots.
pub type PtrCompareFunc = dyn Fn(*const GPointer, *const GPointer) -> i32;

struct PtrArrayData {
    /// Element storage; `None` when length is zero and nothing has been allocated yet.
    pdata: Option<Vec<GPointer>>,
    len: u32,
    alloc: u32,
}

struct PtrArrayInner {
    ref_count: AtomicRefCount,
    null_terminated: bool,
    element_free_fn: RefCell<ElementFreeFn>,
    data: RefCell<PtrArrayData>,
}

/// Reference-counted resizable array of pointers (`GPtrArray`).
///
/// Public fields in C map to [`PtrArray::pdata`] and [`PtrArray::len`]. Only
/// [`PtrArray::ref_`] and [`PtrArray::unref`] are thread-safe; all other APIs
/// require external synchronization if shared across threads.
pub struct PtrArray {
    inner: NonNull<PtrArrayInner>,
}

impl Default for PtrArray {
    fn default() -> Self {
        Self::new()
    }
}

impl PtrArray {
    /// Create an empty array (`g_ptr_array_new`).
    pub fn new() -> Self {
        Self::ptr_array_new(0, None, false)
    }

    /// Create an empty array with an element destructor (`g_ptr_array_new_with_free_func`).
    pub fn new_with_free_func(element_free_fn: impl FnMut(GPointer) + 'static) -> Self {
        Self::ptr_array_new(0, Some(Box::new(element_free_fn)), false)
    }

    /// Preallocate capacity without changing length (`g_ptr_array_sized_new`).
    pub fn sized_new(reserved_size: u32) -> Self {
        Self::ptr_array_new(reserved_size, None, false)
    }

    /// Create an array optionally marked null-terminated (`g_ptr_array_new_null_terminated`).
    pub fn new_null_terminated(
        reserved_size: u32,
        element_free_fn: Option<Box<dyn FnMut(GPointer)>>,
        null_terminated: bool,
    ) -> Self {
        Self::ptr_array_new(reserved_size, element_free_fn, null_terminated)
    }

    /// Whether the array was constructed as null-terminated (`g_ptr_array_is_null_terminated`).
    pub fn is_null_terminated(&self) -> bool {
        self.inner().null_terminated
    }

    /// Number of pointers in the array (`GPtrArray.len`).
    pub fn len(&self) -> u32 {
        self.inner().data.borrow().len
    }

    /// Whether the array contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pointer to the element storage (`GPtrArray.pdata`), or `None` when unallocated.
    pub fn pdata(&self) -> Option<*mut GPointer> {
        self.inner()
            .data
            .borrow()
            .pdata
            .as_ref()
            .map(|v| v.as_ptr() as *mut GPointer)
    }

    /// Return the pointer at `index` without bounds checking (`g_ptr_array_index`).
    ///
    /// # Safety
    ///
    /// `index` must be less than [`PtrArray::len`].
    pub unsafe fn index(&self, index: u32) -> GPointer {
        let data = self.inner().data.borrow();
        let pdata = data.pdata.as_ref().expect("pdata must exist when len > 0");
        debug_assert!(data.len > 0);
        *pdata
            .get(index as usize)
            .expect("index out of bounds in PtrArray::index")
    }

    /// Return the pointer at `index`, or `None` when out of bounds.
    pub fn get(&self, index: u32) -> Option<GPointer> {
        let data = self.inner().data.borrow();
        data.pdata
            .as_ref()
            .and_then(|pdata| pdata.get(index as usize).copied())
            .filter(|_| index < data.len)
    }

    /// Append a pointer (`g_ptr_array_add`).
    pub fn add(&self, data: GPointer) {
        let inner = self.inner();
        let mut state = inner.data.borrow_mut();
        Self::maybe_expand(inner.null_terminated, &mut state, 1);
        let slot = state.len as usize;
        state.pdata.as_mut().unwrap()[slot] = data;
        state.len += 1;
        Self::maybe_null_terminate(inner.null_terminated, &mut state);
    }

    /// Set array length, zero-filling new slots or removing trailing elements
    /// (`g_ptr_array_set_size`).
    pub fn set_size(&self, length: i32) {
        assert!(length >= 0, "length must be non-negative");
        let length = length as u32;
        let inner = self.inner();
        let mut state = inner.data.borrow_mut();

        match length.cmp(&state.len) {
            Ordering::Greater => {
                let old_len = state.len;
                Self::maybe_expand(inner.null_terminated, &mut state, length - old_len);
                let start = old_len as usize;
                let end = length as usize;
                let pdata = state.pdata.as_mut().unwrap();
                for slot in &mut pdata[start..end] {
                    *slot = core::ptr::null_mut();
                }
                state.len = length;
                Self::maybe_null_terminate(inner.null_terminated, &mut state);
            }
            Ordering::Less => {
                let remove_len = state.len - length;
                drop(state);
                self.remove_range(length, remove_len);
            }
            Ordering::Equal => {}
        }
    }

    /// Remove the pointer at `index`, preserving order (`g_ptr_array_remove_index`).
    pub fn remove_index(&self, index: u32) -> Option<GPointer> {
        self.remove_index_internal(index, false, true)
    }

    /// Remove the pointer at `index`, moving the last element into its place
    /// (`g_ptr_array_remove_index_fast`).
    pub fn remove_index_fast(&self, index: u32) -> Option<GPointer> {
        self.remove_index_internal(index, true, true)
    }

    /// Remove the first occurrence of `data`, preserving order (`g_ptr_array_remove`).
    pub fn remove(&self, data: GPointer) -> bool {
        let len = self.len();
        for i in 0..len {
            // SAFETY: i < len.
            if unsafe { self.index(i) } == data {
                self.remove_index(i);
                return true;
            }
        }
        false
    }

    /// Remove `length` pointers starting at `index` (`g_ptr_array_remove_range`).
    pub fn remove_range(&self, index: u32, length: u32) -> &Self {
        let inner = self.inner();
        let mut state = inner.data.borrow_mut();

        assert!(index <= state.len);
        assert!(
            length == 0
                || index
                    .checked_add(length)
                    .is_some_and(|end| end <= state.len)
        );

        if length == 0 {
            return self;
        }

        if let Some(ref mut free_fn) = *inner.element_free_fn.borrow_mut() {
            let pdata = state.pdata.as_ref().unwrap();
            for i in index..index + length {
                free_fn(pdata[i as usize]);
            }
        }

        let tail_start = (index + length) as usize;
        let tail_len = (state.len - index - length) as usize;
        let pdata = state.pdata.as_mut().unwrap();
        if tail_len > 0 {
            pdata.copy_within(tail_start..tail_start + tail_len, index as usize);
        }

        state.len -= length;
        Self::maybe_null_terminate(inner.null_terminated, &mut state);
        self
    }

    /// Stable sort using a `qsort`-style comparator on pointers-to-slots
    /// (`g_ptr_array_sort`).
    pub fn sort(&self, compare_func: &PtrCompareFunc) {
        let inner = self.inner();
        let mut state = inner.data.borrow_mut();
        let len = state.len as usize;
        if len == 0 {
            return;
        }
        let pdata = state.pdata.as_mut().unwrap();
        pdata[..len]
            .sort_by(|a, b| compare_func(a as *const GPointer, b as *const GPointer).cmp(&0));
        Self::maybe_null_terminate(inner.null_terminated, &mut state);
    }

    /// Call `func` for each element (`g_ptr_array_foreach`).
    pub fn foreach<U>(&self, mut func: impl FnMut(GPointer, &mut U), user_data: &mut U) {
        let len = self.len();
        for i in 0..len {
            // SAFETY: i < len.
            let value = unsafe { self.index(i) };
            func(value, user_data);
        }
    }

    /// Find the first pointer-equal element (`g_ptr_array_find`).
    pub fn find(&self, needle: *const (), index_out: Option<&mut u32>) -> bool {
        self.find_with_equal_func(needle, None, index_out)
    }

    /// Find the first element matching `needle` using optional equality (`g_ptr_array_find_with_equal_func`).
    pub fn find_with_equal_func(
        &self,
        needle: *const (),
        equal_func: Option<&dyn Fn(GPointer, *const ()) -> bool>,
        index_out: Option<&mut u32>,
    ) -> bool {
        let len = self.len();
        let equal =
            equal_func.unwrap_or(&|element, target| core::ptr::eq(element as *const (), target));

        for i in 0..len {
            // SAFETY: i < len.
            let element = unsafe { self.index(i) };
            if equal(element, needle) {
                if let Some(index) = index_out {
                    *index = i;
                }
                return true;
            }
        }
        false
    }

    /// Increase the reference count (`g_ptr_array_ref`).
    #[must_use]
    pub fn ref_(&self) -> Self {
        self.inner().ref_count.inc();
        Self { inner: self.inner }
    }

    /// Decrease the reference count, destroying the array at zero (`g_ptr_array_unref`).
    pub fn unref(self) {
        let this = ManuallyDrop::new(self);
        unsafe {
            if (*this.inner.as_ptr()).ref_count.dec() {
                let _ = release_ptr_array(this.inner, true, false);
            }
        }
    }

    /// Release the array (`g_ptr_array_free`).
    ///
    /// When `free_segment` is `true`, element storage is destroyed. Otherwise the
    /// underlying pointer buffer is returned for the caller to release with
    /// [`crate::mem::free`].
    pub fn free(self, free_segment: bool) -> Option<Vec<GPointer>> {
        let this = ManuallyDrop::new(self);
        unsafe {
            let preserve_wrapper = !(*this.inner.as_ptr()).ref_count.dec();
            release_ptr_array(this.inner, free_segment, preserve_wrapper)
        }
    }

    fn ptr_array_new(
        reserved_size: u32,
        element_free_fn: Option<Box<dyn FnMut(GPointer)>>,
        null_terminated: bool,
    ) -> Self {
        let inner = Box::new(PtrArrayInner {
            ref_count: AtomicRefCount::new(),
            null_terminated,
            element_free_fn: RefCell::new(element_free_fn),
            data: RefCell::new(PtrArrayData {
                pdata: None,
                len: 0,
                alloc: 0,
            }),
        });
        let inner = NonNull::new(Box::into_raw(inner)).unwrap();
        let array = Self { inner };

        if reserved_size > 0 {
            let mut state = array.inner().data.borrow_mut();
            Self::maybe_expand(null_terminated, &mut state, reserved_size);
            if null_terminated {
                state.pdata.as_mut().unwrap()[0] = core::ptr::null_mut();
            }
        }

        array
    }

    fn inner(&self) -> &PtrArrayInner {
        unsafe { self.inner.as_ref() }
    }

    fn maybe_null_terminate(null_terminated: bool, state: &mut PtrArrayData) {
        if null_terminated {
            if let Some(pdata) = state.pdata.as_mut() {
                pdata[state.len as usize] = core::ptr::null_mut();
            }
        }
    }

    fn max_len(null_terminated: bool) -> u32 {
        let by_bytes = usize::MAX / 2 / core::mem::size_of::<GPointer>();
        let capped = (by_bytes as u64).min(u32::MAX as u64) as u32;
        if null_terminated {
            capped.saturating_sub(1)
        } else {
            capped
        }
    }

    fn maybe_expand(null_terminated: bool, state: &mut PtrArrayData, len: u32) {
        let max_len = Self::max_len(null_terminated);
        if max_len - state.len < len {
            panic!("adding {len} to array would overflow");
        }

        let extra = if null_terminated { 1u32 } else { 0 };
        let want_len = state
            .len
            .checked_add(len)
            .and_then(|n| n.checked_add(extra))
            .expect("array length overflow");

        if want_len <= state.alloc {
            return;
        }

        let want_bytes = checked_mul_size(want_len as usize, core::mem::size_of::<GPointer>())
            .expect("array allocation overflow");
        let want_alloc_bytes = nearest_pow(want_bytes)
            .max(MIN_ARRAY_CAPACITY as usize * core::mem::size_of::<GPointer>());
        let new_alloc = (want_alloc_bytes / core::mem::size_of::<GPointer>()) as u32;

        match &mut state.pdata {
            Some(pdata) => {
                if (new_alloc as usize) > pdata.capacity() {
                    pdata.reserve_exact(new_alloc as usize - pdata.capacity());
                }
                pdata.resize(new_alloc as usize, core::ptr::null_mut());
            }
            None => {
                state.pdata = Some(vec![core::ptr::null_mut(); new_alloc as usize]);
            }
        }
        state.alloc = new_alloc;
    }

    fn remove_index_internal(
        &self,
        index: u32,
        fast: bool,
        free_element: bool,
    ) -> Option<GPointer> {
        let inner = self.inner();
        let mut state = inner.data.borrow_mut();

        if index >= state.len {
            return None;
        }

        let result = state.pdata.as_ref().unwrap()[index as usize];

        if free_element {
            if let Some(ref mut free_fn) = *inner.element_free_fn.borrow_mut() {
                free_fn(result);
            }
        }

        let last = state.len - 1;
        let pdata = state.pdata.as_mut().unwrap();
        if index != last && !fast {
            pdata.copy_within((index + 1) as usize..=last as usize, index as usize);
        } else if index != last {
            pdata[index as usize] = pdata[last as usize];
        }

        state.len -= 1;
        Self::maybe_null_terminate(inner.null_terminated, &mut state);
        Some(result)
    }
}

impl Clone for PtrArray {
    fn clone(&self) -> Self {
        self.ref_()
    }
}

impl Drop for PtrArray {
    fn drop(&mut self) {
        unsafe {
            if (*self.inner.as_ptr()).ref_count.dec() {
                let _ = release_ptr_array(self.inner, true, false);
            }
        }
    }
}

unsafe fn release_ptr_array(
    inner: NonNull<PtrArrayInner>,
    free_segment: bool,
    preserve_wrapper: bool,
) -> Option<Vec<GPointer>> {
    unsafe {
        let inner_ref = inner.as_ptr();
        let null_terminated = (*inner_ref).null_terminated;
        let segment = {
            let mut state = (*inner_ref).data.borrow_mut();
            if free_segment {
                if let Some(pdata) = state.pdata.take() {
                    if let Some(ref mut free_fn) = *(*inner_ref).element_free_fn.borrow_mut() {
                        for i in 0..state.len {
                            free_fn(pdata[i as usize]);
                        }
                    }
                }
                None
            } else if let Some(pdata) = state.pdata.take() {
                Some(pdata)
            } else if null_terminated {
                Some(vec![core::ptr::null_mut()])
            } else {
                None
            }
        };

        if preserve_wrapper {
            let mut state = (*inner_ref).data.borrow_mut();
            state.len = 0;
            state.alloc = 0;
            state.pdata = None;
        } else {
            drop(Box::from_raw(inner.as_ptr()));
        }

        segment
    }
}

fn checked_mul_size(a: usize, b: usize) -> Option<usize> {
    crate::checked::checked_mul_size(a, b)
}

fn nearest_pow(num: usize) -> usize {
    assert!(num > 0 && num <= usize::MAX / 2);
    let mut n = num - 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    if usize::BITS == 64 {
        n |= n >> 32;
    }
    n + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn int_to_ptr(value: i32) -> GPointer {
        value as isize as GPointer
    }

    fn ptr_to_int(value: GPointer) -> i32 {
        value as isize as i32
    }

    fn counting_free_fn(counter: Arc<AtomicU32>) -> impl FnMut(GPointer) + 'static {
        move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn assert_null_terminated(array: &PtrArray, expected: bool) {
        assert_eq!(array.is_null_terminated(), expected);
        if let Some(pdata) = array.pdata() {
            if expected {
                unsafe {
                    assert!((*pdata.add(array.len() as usize)).is_null());
                }
            }
        } else {
            assert_eq!(array.len(), 0);
        }
    }

    #[test]
    fn new_add_and_index() {
        let array = PtrArray::sized_new(1000);
        for i in 0..10_000 {
            array.add(int_to_ptr(i));
        }
        assert_eq!(array.len(), 10_000);
        for i in 0..10_000 {
            unsafe {
                assert_eq!(ptr_to_int(array.index(i)), i as i32);
            }
        }
    }

    #[test]
    fn add_foreach_and_free_segment() {
        let array = PtrArray::sized_new(1000);
        let mut sum = 0i32;
        for i in 0..10_000 {
            array.add(int_to_ptr(i));
        }
        array.foreach(|value, sum| *sum += ptr_to_int(value), &mut sum);
        assert_eq!(sum, 49_995_000);

        let segment = array.free(false).expect("segment");
        for (i, value) in segment.iter().take(10_000).enumerate() {
            assert_eq!(ptr_to_int(*value), i as i32);
        }
        drop(segment);
    }

    #[test]
    fn free_null_terminated_empty_segment() {
        let array = PtrArray::new_null_terminated(0, None, true);
        assert_null_terminated(&array, true);

        let segment = array.free(false).expect("null-terminated segment");
        assert_eq!(segment.len(), 1);
        assert!(segment[0].is_null());
    }

    #[test]
    fn ref_count_and_free_with_extra_ref() {
        let array = PtrArray::new_null_terminated(0, None, true);
        for i in 0..10_000 {
            array.add(int_to_ptr(i));
            assert_null_terminated(&array, true);
        }

        let mut sum = 0i32;
        array.foreach(|value, sum| *sum += ptr_to_int(value), &mut sum);
        assert_eq!(sum, 49_995_000);

        let survivor = array.ref_();
        array.free(true);

        assert_eq!(survivor.len(), 0);
        assert_null_terminated(&survivor, true);
        survivor.unref();
    }

    #[test]
    fn free_func_on_remove_and_set_size() {
        let counter = Arc::new(AtomicU32::new(0));

        counter.store(0, Ordering::SeqCst);
        PtrArray::new_with_free_func(counting_free_fn(Arc::clone(&counter))).unref();
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        counter.store(0, Ordering::SeqCst);
        let array = PtrArray::new_with_free_func(counting_free_fn(Arc::clone(&counter)));
        array.add(int_to_ptr(1));
        array.add(int_to_ptr(2));
        array.add(int_to_ptr(3));
        array.remove_index(0);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        array.remove_index_fast(1);
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        let frob = int_to_ptr(99);
        array.add(frob);
        assert!(array.remove(frob));
        assert!(!array.remove(int_to_ptr(1234)));
        assert_eq!(counter.load(Ordering::SeqCst), 3);

        array.add(int_to_ptr(77));
        array.set_size(1);
        assert_eq!(counter.load(Ordering::SeqCst), 4);

        let extra = array.ref_();
        extra.unref();
        assert_eq!(counter.load(Ordering::SeqCst), 4);
        array.unref();
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn sort_ascending() {
        let array = PtrArray::new();
        array.sort(&|_, _| 0);

        let values = [5_i32, 1, 9, 3, 7, 2, 8, 0, 4, 6];
        for value in values {
            array.add(int_to_ptr(value));
        }

        array.sort(&|a, b| {
            let av = ptr_to_int(unsafe { *a });
            let bv = ptr_to_int(unsafe { *b });
            av.cmp(&bv) as i32
        });

        let mut prev = i32::MIN;
        for i in 0..array.len() {
            let cur = unsafe { ptr_to_int(array.index(i)) };
            assert!(prev <= cur);
            prev = cur;
        }
    }

    #[test]
    fn find_empty_array() {
        let array = PtrArray::new();
        let mut idx = u32::MAX;
        assert!(!array.find(core::ptr::null(), None));
        assert!(!array.find(core::ptr::null(), Some(&mut idx)));
        assert!(!array.find_with_equal_func(core::ptr::null(), Some(&|_, _| true), Some(&mut idx)));
    }

    #[test]
    fn find_non_empty_array() {
        let array = PtrArray::new();
        let values = ["some", "random", "values", "some", "duplicated"];
        for value in values {
            array.add(value.as_ptr() as GPointer);
        }
        let static_string = "static-string";
        array.add(static_string.as_ptr() as GPointer);

        let mut idx = u32::MAX;
        assert!(array.find_with_equal_func(
            "random".as_ptr() as *const (),
            Some(&|element, target| { element as *const u8 == target as *const u8 }),
            Some(&mut idx)
        ));
        assert_eq!(idx, 1);

        assert!(array.find_with_equal_func(
            "some".as_ptr() as *const (),
            Some(&|element, target| element as *const u8 == target as *const u8),
            Some(&mut idx)
        ));
        assert_eq!(idx, 0);

        assert!(!array.find_with_equal_func(
            "nope".as_ptr() as *const (),
            Some(&|element, target| element as *const u8 == target as *const u8),
            None
        ));

        idx = u32::MAX;
        assert!(array.find(static_string.as_ptr() as *const (), Some(&mut idx)));
        assert_eq!(idx, 5);
    }

    #[test]
    fn remove_range_empty() {
        let array = PtrArray::new();
        array.remove_range(0, 0);
        array.unref();
    }

    #[test]
    fn remove_index_preserves_order() {
        let array = PtrArray::new();
        for i in 0..5 {
            array.add(int_to_ptr(i));
        }
        assert_eq!(ptr_to_int(array.remove_index(1).unwrap()), 1);
        for (i, expected) in [0, 2, 3, 4].iter().enumerate() {
            unsafe {
                assert_eq!(ptr_to_int(array.index(i as u32)), *expected);
            }
        }
    }

    #[test]
    fn remove_index_fast_swaps_last() {
        let array = PtrArray::new();
        for i in 0..4 {
            array.add(int_to_ptr(i));
        }
        assert_eq!(ptr_to_int(array.remove_index_fast(0).unwrap()), 0);
        unsafe {
            assert_eq!(ptr_to_int(array.index(0)), 3);
            assert_eq!(ptr_to_int(array.index(1)), 1);
            assert_eq!(ptr_to_int(array.index(2)), 2);
        }
    }

    #[test]
    fn set_size_grows_and_shrinks() {
        let counter = Arc::new(AtomicU32::new(0));
        let array = PtrArray::new_with_free_func(counting_free_fn(Arc::clone(&counter)));
        array.set_size(4);
        assert_eq!(array.len(), 4);
        for i in 0..4 {
            unsafe {
                assert!(array.index(i).is_null());
            }
        }

        array.set_size(0);
        assert_eq!(array.len(), 0);

        array.add(int_to_ptr(10));
        array.add(int_to_ptr(20));
        counter.store(0, Ordering::SeqCst);
        array.set_size(1);
        assert_eq!(array.len(), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        unsafe {
            assert_eq!(ptr_to_int(array.index(0)), 10);
        }
        array.unref();
    }

    #[test]
    fn null_terminated_add_remove() {
        let array = PtrArray::new_null_terminated(4, None, true);
        assert_null_terminated(&array, true);
        array.add(int_to_ptr(1));
        array.add(int_to_ptr(2));
        assert_null_terminated(&array, true);
        array.remove_index(0);
        assert_null_terminated(&array, true);
        unsafe {
            assert_eq!(ptr_to_int(array.index(0)), 2);
        }
        array.unref();
    }
}
