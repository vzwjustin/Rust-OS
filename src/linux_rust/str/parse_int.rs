//! Integer parsing functions.
//!
//! Ported from Linux `rust/kernel/str/parse_int.rs`.

use super::BStr;
use core::ops::Deref;

/// Error type for parse operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError;

/// Sealed trait for radix-based parsing.
mod private {
    use super::BStr;

    pub trait FromStrRadix: Sized {
        fn from_str_radix(src: &BStr, radix: u32) -> Result<Self, super::ParseError>;
        fn from_u64_negated(value: u64) -> Result<Self, super::ParseError>;
    }
}

/// Extract the radix from an integer literal optionally prefixed with
/// one of `0x`, `0X`, `0o`, `0O`, `0b`, `0B`, `0`.
fn strip_radix(src: &BStr) -> (u32, &BStr) {
    match src.deref() {
        [b'0', b'x' | b'X', rest @ ..] => (16, BStr::from_bytes(rest)),
        [b'0', b'o' | b'O', rest @ ..] => (8, BStr::from_bytes(rest)),
        [b'0', b'b' | b'B', rest @ ..] => (2, BStr::from_bytes(rest)),
        [b'0', ..] => (8, src),
        _ => (10, src),
    }
}

/// Trait for parsing string representations of integers.
///
/// Strings beginning with `0x`, `0o`, or `0b` are parsed as hex, octal, or
/// binary respectively. Strings beginning with `0` otherwise are parsed as
/// octal. Anything else is parsed as decimal. A leading `+` or `-` is also
/// permitted.
pub trait ParseInt: private::FromStrRadix + TryFrom<u64> {
    /// Parse a string as an integer.
    fn from_str(src: &BStr) -> Result<Self, ParseError> {
        match src.deref() {
            [b'-', rest @ ..] => {
                let (radix, digits) = strip_radix(BStr::from_bytes(rest));
                let val = u64::from_str_radix(
                    core::str::from_utf8(digits).map_err(|_| ParseError)?,
                    radix,
                )
                .map_err(|_| ParseError)?;
                Self::from_u64_negated(val)
            }
            [b'+', rest @ ..] => {
                let (radix, digits) = strip_radix(BStr::from_bytes(rest));
                Self::from_str_radix(digits, radix)
            }
            _ => {
                let (radix, digits) = strip_radix(src);
                Self::from_str_radix(digits, radix)
            }
        }
    }
}

macro_rules! impl_parse_int {
    ($($ty:ty),*) => {
        $(
            impl private::FromStrRadix for $ty {
                fn from_str_radix(src: &BStr, radix: u32) -> Result<Self, ParseError> {
                    <$ty>::from_str_radix(
                        core::str::from_utf8(src).map_err(|_| ParseError)?,
                        radix,
                    ).map_err(|_| ParseError)
                }

                fn from_u64_negated(value: u64) -> Result<Self, ParseError> {
                    const ABS_MIN: u64 = {
                        #[allow(unused_comparisons)]
                        if <$ty>::MIN < 0 {
                            1u64 << (<$ty>::BITS - 1)
                        } else {
                            0
                        }
                    };

                    if value > ABS_MIN {
                        return Err(ParseError);
                    }

                    if value == ABS_MIN {
                        return Ok(<$ty>::MIN);
                    }

                    let value: Self = value.try_into().map_err(|_| ParseError)?;
                    Ok((!value).wrapping_add(1))
                }
            }

            impl ParseInt for $ty {}
        )*
    };
}

impl_parse_int![i8, u8, i16, u16, i32, u32, i64, u64, isize, usize];

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_parse_basic() {
        assert_eq!(
            u8::from_str(BStr::from_bytes(b"0")),
            Ok(0u8)
        );
        assert_eq!(
            u8::from_str(BStr::from_bytes(b"0xa2")),
            Ok(0xa2u8)
        );
        assert_eq!(
            i32::from_str(BStr::from_bytes(b"-0xa2")),
            Ok(-0xa2i32)
        );
        assert_eq!(
            i8::from_str(BStr::from_bytes(b"127")),
            Ok(127i8)
        );
        assert!(i8::from_str(BStr::from_bytes(b"128")).is_err());
        assert_eq!(
            i8::from_str(BStr::from_bytes(b"-128")),
            Ok(-128i8)
        );
        assert!(i8::from_str(BStr::from_bytes(b"-129")).is_err());
    }
}
