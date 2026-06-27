//! `giroffsets` matching `girepository/giroffsets.c`.
//!
//! Offset computation: calculates field offsets for structs and unions
//! used by the typelib writer.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

/// Computes the alignment of a type given its size
/// (mirrors offset alignment logic in giroffsets.c).
pub fn align_to(size: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return size;
    }
    (size + alignment - 1) & !(alignment - 1)
}

/// Computes the offset of a field at a given alignment.
pub fn field_offset(current_offset: usize, field_alignment: usize) -> usize {
    align_to(current_offset, field_alignment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_to() {
        assert_eq!(align_to(0, 4), 0);
        assert_eq!(align_to(1, 4), 4);
        assert_eq!(align_to(3, 4), 4);
        assert_eq!(align_to(4, 4), 4);
        assert_eq!(align_to(5, 4), 8);
        assert_eq!(align_to(8, 8), 8);
    }

    #[test]
    fn test_field_offset() {
        assert_eq!(field_offset(0, 4), 0);
        assert_eq!(field_offset(1, 4), 4);
        assert_eq!(field_offset(5, 8), 8);
    }

    #[test]
    fn test_align_zero() {
        assert_eq!(align_to(5, 0), 5);
    }
}
