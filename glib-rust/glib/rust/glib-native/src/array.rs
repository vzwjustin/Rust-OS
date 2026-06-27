//! Growable array types matching `garray.h` / `garray.c`.

use crate::checked::checked_mul_size;
use crate::mem::realloc;
use crate::prelude::*;
use crate::refcount::AtomicRefCount;
use crate::UInt;
use alloc::sync::Arc;
use core::cmp::Ordering;
use core::fmt;
use spin::mutex::{Mutex, MutexGuard};

const MIN_ARRAY_SIZE: usize = 16;

struct GArrayState {
    data: Vec<u8>,
    len: UInt,
    elt_capacity: UInt,
    elt_size: UInt,
    zero_terminated: bool,
    clear: bool,
    max_len: UInt,
}

struct GArrayInner {
    state: Mutex<GArrayState>,
    ref_count: AtomicRefCount,
}

/// Generic growable array (`GArray`).
pub struct GArray {
    inner: Arc<GArrayInner>,
}

impl Clone for GArray {
    fn clone(&self) -> Self {
        self.ref_()
    }
}

impl Drop for GArray {
    fn drop(&mut self) {
        self.inner.ref_count.dec();
    }
}

impl GArray {
    fn state(&self) -> MutexGuard<'_, GArrayState> {
        self.inner.state.lock()
    }

    /// Create a new array (`g_array_new`).
    pub fn new(zero_terminated: bool, clear: bool, element_size: UInt) -> Self {
        assert!(element_size > 0, "element_size must be > 0");
        Self::sized_new(zero_terminated, clear, element_size, 0)
    }

    /// Create with preallocated capacity (`g_array_sized_new`).
    pub fn sized_new(
        zero_terminated: bool,
        clear: bool,
        element_size: UInt,
        reserved_size: UInt,
    ) -> Self {
        assert!(element_size > 0, "element_size must be > 0");

        let elt_size = element_size as usize;
        let max_from_bytes = usize::MAX / 2 / elt_size;
        let max_from_uint = u32::MAX as usize;
        let max_len = max_from_bytes
            .min(max_from_uint)
            .saturating_sub(usize::from(zero_terminated)) as UInt;

        let array = Self {
            inner: Arc::new(GArrayInner {
                state: Mutex::new(GArrayState {
                    data: Vec::new(),
                    len: 0,
                    elt_capacity: 0,
                    elt_size: element_size,
                    zero_terminated,
                    clear,
                    max_len,
                }),
                ref_count: AtomicRefCount::new(),
            }),
        };

        if zero_terminated || reserved_size != 0 {
            array.maybe_expand(reserved_size);
            array.zero_terminate();
        }

        array
    }

    /// Element count (`len` field).
    pub fn len(&self) -> UInt {
        self.state().len
    }

    /// Whether the array contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Element size in bytes (`g_array_get_element_size`).
    pub fn element_size(&self) -> UInt {
        self.state().elt_size
    }

    /// Raw element storage (`data` field).
    pub fn data(&self) -> Vec<u8> {
        let state = self.state();
        let byte_len = elt_byte_len(&state, state.len);
        state.data[..byte_len].to_vec()
    }

    /// Read element `index` as `i32` (mirrors `g_array_index` for `gint`).
    pub fn index_i32(&self, index: UInt) -> i32 {
        let state = self.state();
        assert!((index as usize) < state.len as usize, "index out of bounds");
        let offset = elt_pos(&state, index);
        let size = state.elt_size as usize;
        match size {
            1 => state.data[offset] as i8 as i32,
            2 => {
                let mut bytes = [0u8; 2];
                bytes.copy_from_slice(&state.data[offset..offset + 2]);
                i16::from_ne_bytes(bytes) as i32
            }
            _ => {
                let mut bytes = [0u8; 4];
                let copy_len = size.min(4);
                bytes[..copy_len].copy_from_slice(&state.data[offset..offset + copy_len]);
                i32::from_ne_bytes(bytes)
            }
        }
    }

    /// Increase reference count (`g_array_ref`).
    #[must_use]
    pub fn ref_(&self) -> Self {
        self.inner.ref_count.inc();
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Drop a reference (`g_array_unref`).
    pub fn unref(self) {
        drop(self);
    }

    /// Shallow copy (`g_array_copy`).
    pub fn copy(&self) -> Self {
        let state = self.state();
        let copy = Self::sized_new(
            state.zero_terminated,
            state.clear,
            state.elt_size,
            state.len,
        );
        {
            let mut copy_state = copy.state();
            copy_state.len = state.len;
            let byte_len = elt_byte_len(&state, state.len);
            if byte_len > 0 {
                copy_state.data.resize(byte_len, 0);
                copy_state.data[..byte_len].copy_from_slice(&state.data[..byte_len]);
                copy_state.elt_capacity = capacity_in_elements(&copy_state);
            }
        }
        copy.zero_terminate();
        copy
    }

    /// Free the array (`g_array_free`).
    ///
    /// When `free_segment` is `false`, returns the element data buffer. When
    /// other references exist, the wrapper is preserved and its length reset.
    pub fn free(self, free_segment: bool) -> Option<Vec<u8>> {
        let preserve = Arc::strong_count(&self.inner) > 1;
        let segment = {
            let mut state = self.state();
            if free_segment {
                state.data.clear();
                None
            } else {
                let mut data = core::mem::take(&mut state.data);
                let mut end = elt_byte_len(&state, state.len);
                if state.zero_terminated {
                    end += state.elt_size as usize;
                }
                data.truncate(end);
                Some(data)
            }
        };
        if preserve {
            let mut state = self.state();
            state.len = 0;
            state.elt_capacity = 0;
        }
        segment
    }

    /// Append elements (`g_array_append_vals`).
    pub fn append_vals(&self, data: Option<&[u8]>, len: UInt) -> &Self {
        if len == 0 {
            return self;
        }
        let data = data.expect("data must be non-null when len > 0");
        self.maybe_expand(len);
        {
            let mut state = self.state();
            let dst = elt_pos(&state, state.len);
            let copy_len = elt_byte_len(&state, len);
            assert!(data.len() >= copy_len, "data too short for append");
            state.data[dst..dst + copy_len].copy_from_slice(&data[..copy_len]);
            state.len = state.len.checked_add(len).expect("array length overflow");
        }
        self.zero_terminate();
        self
    }

    /// Prepend elements (`g_array_prepend_vals`).
    pub fn prepend_vals(&self, data: Option<&[u8]>, len: UInt) -> &Self {
        if len == 0 {
            return self;
        }
        let data = data.expect("data must be non-null when len > 0");
        self.maybe_expand(len);
        {
            let mut state = self.state();
            let old_byte_len = elt_byte_len(&state, state.len);
            let new_byte_len = elt_byte_len(&state, len);
            let dst = new_byte_len;
            state.data.resize(old_byte_len + new_byte_len, 0);
            state.data.copy_within(0..old_byte_len, dst);
            state.data[..new_byte_len].copy_from_slice(&data[..new_byte_len]);
            state.len = state.len.checked_add(len).expect("array length overflow");
        }
        self.zero_terminate();
        self
    }

    /// Insert elements at `index` (`g_array_insert_vals`).
    pub fn insert_vals(&self, index: UInt, data: Option<&[u8]>, len: UInt) -> &Self {
        if len == 0 {
            return self;
        }
        let data = data.expect("data must be non-null when len > 0");

        let cur_len = self.len();
        if index >= cur_len {
            let gap = index - cur_len;
            self.maybe_expand(gap.checked_add(len).expect("overflow"));
            self.set_size(index);
            return self.append_vals(Some(data), len);
        }

        self.maybe_expand(len);
        {
            let mut state = self.state();
            let tail = cur_len - index;
            let insert_bytes = elt_byte_len(&state, len);
            let tail_bytes = elt_byte_len(&state, tail);
            let index_bytes = elt_pos(&state, index);
            let new_data_len = state.data.len() + insert_bytes;

            state.data.resize(new_data_len, 0);
            state.data.copy_within(
                index_bytes..index_bytes + tail_bytes,
                index_bytes + insert_bytes,
            );
            state.data[index_bytes..index_bytes + insert_bytes]
                .copy_from_slice(&data[..insert_bytes]);
            state.len = cur_len.checked_add(len).expect("array length overflow");
        }
        self.zero_terminate();
        self
    }

    /// Set element count (`g_array_set_size`).
    pub fn set_size(&self, length: UInt) -> &Self {
        let cur_len = self.len();
        match length.cmp(&cur_len) {
            Ordering::Greater => {
                self.maybe_expand(length - cur_len);
                let mut state = self.state();
                if state.clear {
                    let start = elt_pos(&state, cur_len);
                    let end = elt_pos(&state, length);
                    state.data[start..end].fill(0);
                }
            }
            Ordering::Less => {
                self.remove_range(length, cur_len - length);
            }
            Ordering::Equal => {}
        }
        self.state().len = length;
        self.zero_terminate();
        self
    }

    /// Remove one element preserving order (`g_array_remove_index`).
    pub fn remove_index(&self, index: UInt) -> &Self {
        let len = self.len();
        assert!((index as usize) < len as usize, "index out of bounds");

        {
            let mut state = self.state();
            if index != len - 1 {
                let dst = elt_pos(&state, index);
                let src = elt_pos(&state, index + 1);
                let move_len = elt_byte_len(&state, len - index - 1);
                state.data.copy_within(src..src + move_len, dst);
            }
            state.len -= 1;
            truncate_data(&mut state);
        }
        self.zero_terminate();
        self
    }

    /// Remove one element without preserving order (`g_array_remove_index_fast`).
    pub fn remove_index_fast(&self, index: UInt) -> &Self {
        let len = self.len();
        assert!((index as usize) < len as usize, "index out of bounds");

        {
            let mut state = self.state();
            if index != len - 1 {
                let dst = elt_pos(&state, index);
                let src = elt_pos(&state, len - 1);
                let elt_bytes = state.elt_size as usize;
                let last = state.data[src..src + elt_bytes].to_vec();
                state.data[dst..dst + elt_bytes].copy_from_slice(&last);
            }
            state.len -= 1;
            truncate_data(&mut state);
        }
        self.zero_terminate();
        self
    }

    /// Remove a span of elements (`g_array_remove_range`).
    pub fn remove_range(&self, index: UInt, length: UInt) -> &Self {
        let len = self.len();
        assert!(index <= len, "index out of bounds");
        assert!(
            index as u64 + length as u64 <= u32::MAX as u64,
            "length overflow"
        );
        assert!(index + length <= len, "range out of bounds");

        if length == 0 {
            return self;
        }

        {
            let mut state = self.state();
            if index + length != len {
                let dst = elt_pos(&state, index);
                let src = elt_pos(&state, index + length);
                let move_len = elt_byte_len(&state, len - index - length);
                state.data.copy_within(src..src + move_len, dst);
            }
            state.len -= length;
            truncate_data(&mut state);
        }
        self.zero_terminate();
        self
    }

    fn maybe_expand(&self, len: UInt) {
        let mut state = self.state();
        maybe_expand_state(&mut state, len);
    }

    fn zero_terminate(&self) {
        let mut state = self.state();
        if !state.zero_terminated {
            return;
        }
        let term_start = elt_pos(&state, state.len);
        let term_len = state.elt_size as usize;
        if state.data.len() < term_start + term_len {
            state.data.resize(term_start + term_len, 0);
        } else {
            state.data[term_start..term_start + term_len].fill(0);
        }
    }
}

impl fmt::Debug for GArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GArray")
            .field("len", &self.len())
            .field("element_size", &self.element_size())
            .field("data_len", &self.data().len())
            .finish()
    }
}

/// Type-safe byte array (`GByteArray`).
#[derive(Debug)]
pub struct ByteArray(GArray);

impl Clone for ByteArray {
    fn clone(&self) -> Self {
        Self(self.0.ref_())
    }
}

impl ByteArray {
    /// Create an empty byte array (`g_byte_array_new`).
    pub fn new() -> Self {
        Self(GArray::sized_new(false, false, 1, 0))
    }

    /// Create with reserved capacity (`g_byte_array_sized_new`).
    pub fn sized_new(reserved_size: UInt) -> Self {
        Self(GArray::sized_new(false, false, 1, reserved_size))
    }

    /// Element count.
    pub fn len(&self) -> UInt {
        self.0.len()
    }

    /// Whether the byte array is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Raw bytes.
    pub fn data(&self) -> Vec<u8> {
        self.0.data()
    }

    /// Increase reference count (`g_byte_array_ref`).
    #[must_use]
    pub fn ref_(&self) -> Self {
        Self(self.0.ref_())
    }

    /// Drop a reference (`g_byte_array_unref`).
    pub fn unref(self) {
        drop(self);
    }

    /// Free the array (`g_byte_array_free`).
    pub fn free(self, free_segment: bool) -> Option<Vec<u8>> {
        self.0.free(free_segment)
    }

    /// Append bytes (`g_byte_array_append`).
    pub fn append(&self, data: &[u8], len: UInt) -> &Self {
        self.0.append_vals(Some(data), len);
        self
    }

    /// Prepend bytes (`g_byte_array_prepend`).
    pub fn prepend(&self, data: &[u8], len: UInt) -> &Self {
        self.0.prepend_vals(Some(data), len);
        self
    }

    /// Set size (`g_byte_array_set_size`).
    pub fn set_size(&self, length: UInt) -> &Self {
        self.0.set_size(length);
        self
    }

    /// Remove byte at index (`g_byte_array_remove_index`).
    pub fn remove_index(&self, index: UInt) -> &Self {
        self.0.remove_index(index);
        self
    }

    /// Remove byte without preserving order (`g_byte_array_remove_index_fast`).
    pub fn remove_index_fast(&self, index: UInt) -> &Self {
        self.0.remove_index_fast(index);
        self
    }

    /// Remove byte range (`g_byte_array_remove_range`).
    pub fn remove_range(&self, index: UInt, length: UInt) -> &Self {
        self.0.remove_range(index, length);
        self
    }
}

impl Default for ByteArray {
    fn default() -> Self {
        Self::new()
    }
}

fn maybe_expand_state(state: &mut GArrayState, len: UInt) {
    if (state.max_len - state.len) < len {
        panic!("adding {len} to array would overflow");
    }

    let want_len = state
        .len
        .checked_add(len)
        .and_then(|n| n.checked_add(u32::from(state.zero_terminated)))
        .expect("array length overflow");

    if want_len > state.elt_capacity {
        let want_bytes = elt_byte_len(state, want_len);
        let want_alloc = nearest_pow(want_bytes).max(MIN_ARRAY_SIZE);
        state.data = realloc(core::mem::take(&mut state.data), want_alloc);
        state.elt_capacity = capacity_in_elements(state);
    }
}

fn truncate_data(state: &mut GArrayState) {
    let byte_len = elt_byte_len(state, state.len);
    if state.zero_terminated {
        let term = state.elt_size as usize;
        state.data.resize(byte_len + term, 0);
    } else {
        state.data.truncate(byte_len);
    }
}

fn capacity_in_elements(state: &GArrayState) -> UInt {
    let elt = state.elt_size as usize;
    if elt == 0 {
        return 0;
    }
    (state.data.len() / elt) as UInt
}

fn elt_byte_len(state: &GArrayState, n_elements: UInt) -> usize {
    checked_mul_size(state.elt_size as usize, n_elements as usize)
        .expect("element byte length overflow")
}

fn elt_pos(state: &GArrayState, index: UInt) -> usize {
    checked_mul_size(state.elt_size as usize, index as usize).expect("element position overflow")
}

fn nearest_pow(num: usize) -> usize {
    assert!(num > 0 && num <= usize::MAX / 2);
    let mut n = num - 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        n |= n >> 32;
    }
    n + 1
}

#[cfg(test)]
fn append_i32(array: &GArray, value: i32) {
    array.append_vals(Some(&value.to_ne_bytes()), 1);
}

#[cfg(test)]
fn prepend_i32(array: &GArray, value: i32) {
    array.prepend_vals(Some(&value.to_ne_bytes()), 1);
}

#[cfg(test)]
fn int_array_from(array: &GArray) -> Vec<i32> {
    (0..array.len()).map(|i| array.index_i32(i)).collect()
}

#[cfg(test)]
fn assert_int_array_equal(array: &GArray, expected: &[i32]) {
    assert_eq!(array.len() as usize, expected.len());
    for (i, &exp) in expected.iter().enumerate() {
        assert_eq!(array.index_i32(i as UInt), exp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_set_size_clears_when_configured() {
        let array = GArray::new(false, true, std::mem::size_of::<i32>() as UInt);
        assert_eq!(array.len(), 0);
        array.set_size(5);
        assert_eq!(array.len(), 5);
        for i in 0..5 {
            assert_eq!(array.index_i32(i), 0);
        }
        array.unref();
    }

    #[test]
    fn array_set_size_sized_new() {
        let array = GArray::sized_new(false, true, std::mem::size_of::<i32>() as UInt, 10);
        assert_eq!(array.len(), 0);
        array.set_size(5);
        assert_eq!(array.len(), 5);
        for i in 0..5 {
            assert_eq!(array.index_i32(i), 0);
        }
        array.unref();
    }

    #[test]
    fn array_new_zero_terminated() {
        let array = GArray::new(true, false, 1);
        array.append_vals(Some(b"hello"), 5);
        assert_eq!(array.len(), 5);
        assert_eq!(array.data(), b"hello".to_vec());
        let data = array.clone().free(false).unwrap();
        assert_eq!(&data[..5], b"hello");
        assert_eq!(data[5], 0);
    }

    #[test]
    fn array_append_vals() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        for i in 0..100u32 {
            append_i32(&array, i as i32);
        }
        assert_eq!(array.len(), 100);
        for i in 0..100 {
            assert_eq!(array.index_i32(i), i as i32);
        }
        let segment = array.clone().free(false).unwrap();
        assert_eq!(segment.len(), 100 * std::mem::size_of::<i32>());
        array.unref();
    }

    #[test]
    fn array_prepend_vals() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        let vals = [0i32, 1, 2, 3, 4];
        array.prepend_vals(Some(i32_slice_bytes(&vals[..2])), 2);
        assert_int_array_equal(&array, &[0, 1]);
        array.prepend_vals(Some(i32_slice_bytes(&vals[2..3])), 1);
        assert_int_array_equal(&array, &[2, 0, 1]);
        array.prepend_vals(Some(i32_slice_bytes(&vals[3..5])), 2);
        assert_int_array_equal(&array, &[3, 4, 2, 0, 1]);
        array.prepend_vals(None, 0);
        assert_int_array_equal(&array, &[3, 4, 2, 0, 1]);
        array.unref();
    }

    #[test]
    fn array_insert_vals() {
        let array = GArray::new(false, true, std::mem::size_of::<i32>() as UInt);
        let vals = [0i32, 1, 2, 3, 4, 5, 6, 7];
        array.insert_vals(0, Some(i32_slice_bytes(&vals[..2])), 2);
        assert_int_array_equal(&array, &[0, 1]);
        array.insert_vals(1, Some(i32_slice_bytes(&vals[2..4])), 2);
        assert_int_array_equal(&array, &[0, 2, 3, 1]);
        array.insert_vals(array.len(), Some(i32_slice_bytes(&vals[4..5])), 1);
        assert_int_array_equal(&array, &[0, 2, 3, 1, 4]);
        array.insert_vals(0, Some(i32_slice_bytes(&vals[5..6])), 1);
        assert_int_array_equal(&array, &[5, 0, 2, 3, 1, 4]);
        array.insert_vals(array.len() + 4, Some(i32_slice_bytes(&vals[6..8])), 2);
        assert_eq!(array.len(), 12);
        assert_eq!(array.index_i32(0), 5);
        assert_eq!(array.index_i32(5), 4);
        assert_eq!(array.index_i32(6), 0);
        assert_eq!(array.index_i32(10), 6);
        assert_eq!(array.index_i32(11), 7);
        array.unref();
    }

    #[test]
    fn array_remove_index() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        for i in 0..100 {
            append_i32(&array, i);
        }
        array.remove_index(1);
        array.remove_index(3);
        array.remove_index(21);
        array.remove_index(57);
        assert_eq!(array.len(), 96);
        let values = int_array_from(&array);
        for w in [1, 4, 23, 60] {
            assert!(!values.contains(&w));
        }
        let mut prev = -1;
        for v in values {
            assert!(prev < v);
            prev = v;
        }
        array.unref();
    }

    #[test]
    fn array_remove_index_fast() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        for i in 0..100 {
            append_i32(&array, i);
        }
        array.remove_index_fast(1);
        array.remove_index_fast(3);
        array.remove_index_fast(21);
        array.remove_index_fast(57);
        assert_eq!(array.len(), 96);
        let values = int_array_from(&array);
        for w in [1, 3, 21, 57] {
            assert!(!values.contains(&w));
        }
        array.unref();
    }

    #[test]
    fn array_remove_range() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        for i in 0..100 {
            append_i32(&array, i);
        }
        array.remove_range(31, 4);
        assert_eq!(array.len(), 96);
        let values = int_array_from(&array);
        for v in values {
            assert!(v < 31 || v > 34);
        }
        array.remove_range(0, array.len());
        assert_eq!(array.len(), 0);
        array.remove_range(0, 0);
        assert_eq!(array.len(), 0);
        array.unref();
    }

    #[test]
    fn array_ref_count() {
        let array = GArray::new(false, false, std::mem::size_of::<i32>() as UInt);
        assert_eq!(array.element_size(), std::mem::size_of::<i32>() as UInt);
        for i in 0..100 {
            prepend_i32(&array, i);
        }
        let array2 = array.ref_();
        assert!(Arc::ptr_eq(&array.inner, &array2.inner));
        array2.unref();
        for i in 0..100 {
            assert_eq!(array.index_i32(i), (100 - i - 1) as i32);
        }
        let array2 = array.ref_();
        array.free(true);
        assert_eq!(array2.len(), 0);
        array2.unref();
    }

    #[test]
    fn array_copy_sized() {
        let array1 = GArray::sized_new(false, false, std::mem::size_of::<i32>() as UInt, 1);
        let array2 = array1.copy();
        assert_eq!(array2.len(), array1.len());
        append_i32(&array1, 5);
        let array3 = array1.copy();
        assert_eq!(array3.len(), array1.len());
        assert_eq!(array3.index_i32(0), array1.index_i32(0));
        assert_eq!(array3.len(), 1);
        assert_eq!(array3.index_i32(0), 5);
        array3.unref();
        array2.unref();
        array1.unref();
    }

    #[test]
    fn array_copy_zero_terminated() {
        let mut array = GArray::new(true, false, 1);
        for _ in 0..32 {
            array = array.copy();
        }
        array.unref();
    }

    #[test]
    fn byte_array_append() {
        let chunk = b"abcd";
        let array = ByteArray::sized_new(1000);
        for _ in 0..1000 {
            array.append(chunk, 4);
        }
        assert_eq!(array.len(), 4000);
        assert_eq!(&array.data()[0..4], chunk);
        let segment = array.clone().free(false).unwrap();
        assert_eq!(segment.len(), 4000);
        array.unref();
    }

    #[test]
    fn byte_array_ref_count() {
        let array = ByteArray::new();
        array.append(b"abcd", 4);
        let array2 = array.ref_();
        array2.unref();
        assert_eq!(array.data(), b"abcd".to_vec());
        let array2 = array.ref_();
        array.free(true);
        assert_eq!(array2.len(), 0);
        array2.unref();
    }

    fn i32_slice_bytes(values: &[i32]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                values.as_ptr() as *const u8,
                values.len() * std::mem::size_of::<i32>(),
            )
        }
    }
}
