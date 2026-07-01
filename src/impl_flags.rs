// SPDX-License-Identifier: GPL-2.0
//! Bitflag type generator — ported from Linux `rust/kernel/impl_flags.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

/// Generate a bitflag struct and its companion flag enum with full operator support.
///
/// # Syntax
///
/// ```rust,ignore
/// impl_flags!(
///     #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
///     pub struct MyFlags(u32);
///
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub enum MyFlag {
///         Read    = 1 << 0,
///         Write   = 1 << 1,
///         Execute = 1 << 2,
///     }
/// );
/// ```
///
/// The macro generates `BitOr`, `BitAnd`, `BitXor`, `Not`, and their
/// assign variants, plus `contains`, `contains_any`, `contains_all`, and
/// `empty` / `all_bits` helpers.
#[macro_export]
macro_rules! impl_flags {
    (
        $(#[$outer_flags:meta])*
        $vis_flags:vis struct $flags:ident($ty:ty);

        $(#[$outer_flag:meta])*
        $vis_flag:vis enum $flag:ident {
            $(
                $(#[$inner_flag:meta])*
                $name:ident = $value:expr
            ),+ $(,)?
        }
    ) => {
        $(#[$outer_flags])*
        #[repr(transparent)]
        $vis_flags struct $flags($ty);

        $(#[$outer_flag])*
        #[repr($ty)]
        $vis_flag enum $flag {
            $(
                $(#[$inner_flag])*
                $name = $value
            ),+
        }

        impl ::core::convert::From<$flag> for $flags {
            #[inline]
            fn from(value: $flag) -> Self { Self(value as $ty) }
        }

        impl ::core::convert::From<$flags> for $ty {
            #[inline]
            fn from(value: $flags) -> Self { value.0 }
        }

        impl ::core::ops::BitOr for $flags {
            type Output = Self;
            #[inline]
            fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }
        impl ::core::ops::BitOrAssign for $flags {
            #[inline]
            fn bitor_assign(&mut self, rhs: Self) { *self = *self | rhs; }
        }
        impl ::core::ops::BitOr<$flag> for $flags {
            type Output = Self;
            #[inline]
            fn bitor(self, rhs: $flag) -> Self { self | Self::from(rhs) }
        }
        impl ::core::ops::BitOrAssign<$flag> for $flags {
            #[inline]
            fn bitor_assign(&mut self, rhs: $flag) { *self = *self | rhs; }
        }

        impl ::core::ops::BitAnd for $flags {
            type Output = Self;
            #[inline]
            fn bitand(self, rhs: Self) -> Self { Self(self.0 & rhs.0) }
        }
        impl ::core::ops::BitAndAssign for $flags {
            #[inline]
            fn bitand_assign(&mut self, rhs: Self) { *self = *self & rhs; }
        }
        impl ::core::ops::BitAnd<$flag> for $flags {
            type Output = Self;
            #[inline]
            fn bitand(self, rhs: $flag) -> Self { self & Self::from(rhs) }
        }
        impl ::core::ops::BitAndAssign<$flag> for $flags {
            #[inline]
            fn bitand_assign(&mut self, rhs: $flag) { *self = *self & rhs; }
        }

        impl ::core::ops::BitXor for $flags {
            type Output = Self;
            #[inline]
            fn bitxor(self, rhs: Self) -> Self {
                Self((self.0 ^ rhs.0) & Self::all_bits())
            }
        }
        impl ::core::ops::BitXorAssign for $flags {
            #[inline]
            fn bitxor_assign(&mut self, rhs: Self) { *self = *self ^ rhs; }
        }
        impl ::core::ops::BitXor<$flag> for $flags {
            type Output = Self;
            #[inline]
            fn bitxor(self, rhs: $flag) -> Self { self ^ Self::from(rhs) }
        }
        impl ::core::ops::BitXorAssign<$flag> for $flags {
            #[inline]
            fn bitxor_assign(&mut self, rhs: $flag) { *self = *self ^ rhs; }
        }

        impl ::core::ops::Not for $flags {
            type Output = Self;
            #[inline]
            fn not(self) -> Self { Self((!self.0) & Self::all_bits()) }
        }

        impl ::core::ops::BitOr for $flag {
            type Output = $flags;
            #[inline]
            fn bitor(self, rhs: Self) -> $flags { $flags(self as $ty | rhs as $ty) }
        }
        impl ::core::ops::BitAnd for $flag {
            type Output = $flags;
            #[inline]
            fn bitand(self, rhs: Self) -> $flags { $flags(self as $ty & rhs as $ty) }
        }
        impl ::core::ops::BitXor for $flag {
            type Output = $flags;
            #[inline]
            fn bitxor(self, rhs: Self) -> $flags {
                $flags((self as $ty ^ rhs as $ty) & $flags::all_bits())
            }
        }
        impl ::core::ops::Not for $flag {
            type Output = $flags;
            #[inline]
            fn not(self) -> $flags { $flags((!(self as $ty)) & $flags::all_bits()) }
        }

        impl $flags {
            /// Returns a flags value with no bits set.
            #[inline]
            pub const fn empty() -> Self { Self(0) }

            /// Returns a bitmask of all valid flag bits OR-ed together.
            #[inline]
            pub const fn all_bits() -> $ty { 0 $( | $value )+ }

            /// Returns `true` if `flag` is set in `self`.
            #[inline]
            pub fn contains(self, flag: $flag) -> bool {
                (self.0 & flag as $ty) == flag as $ty
            }

            /// Returns `true` if at least one bit in `flags` is set in `self`.
            #[inline]
            pub fn contains_any(self, flags: $flags) -> bool {
                (self.0 & flags.0) != 0
            }

            /// Returns `true` if every bit in `flags` is set in `self`.
            #[inline]
            pub fn contains_all(self, flags: $flags) -> bool {
                (self.0 & flags.0) == flags.0
            }
        }
    };
}
