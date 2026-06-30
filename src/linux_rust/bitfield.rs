//! Bitfield support.
//!
//! Ported from Linux `rust/kernel/bitfield.rs` and `rust/kernel/num/bounded.rs`.
//!
//! Provides [`Bounded<T, N>`] — an integer wrapper that guarantees values
//! fit within `N` bits — and the [`bitfield!`] macro for declaring
//! bitfield structs with compile-time and runtime bounds checking.

use core::ops;

use super::num::Integer;

// ── Bounded ──────────────────────────────────────────────────────────────

/// An integer value that requires only the `N` least significant bits.
///
/// # Invariants
/// - `N > 0`
/// - `N <= T::BITS`
/// - Stored values can be represented with at most `N` bits.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bounded<T: Integer, const N: u32>(T);

/// Returns `true` if `value` can be represented with at most `num_bits` bits in `T`.
#[inline(always)]
fn fits_within<T: Integer>(value: T, num_bits: u32) -> bool {
    let shift: u32 = T::BITS - num_bits;
    (value << shift) >> shift == value
}

impl<T: Integer, const N: u32> Bounded<T, N> {
    /// Private constructor enforcing invariants.
    const unsafe fn __new(value: T) -> Self {
        assert!(N != 0);
        assert!(N <= T::BITS);
        Self(value)
    }

    /// Attempts to turn `value` into a `Bounded` using `N` bits.
    /// Returns `None` if `value` doesn't fit within `N` bits.
    pub fn try_new(value: T) -> Option<Self> {
        fits_within(value, N).then(|| {
            // SAFETY: `fits_within` confirmed the value fits.
            unsafe { Self::__new(value) }
        })
    }

    /// Checks that `expr` is valid at compile-time (via build_assert) and builds a new value.
    #[inline(always)]
    pub fn from_expr(expr: T) -> Self {
        crate::build_assert!(
            fits_within(expr, N),
            "Requested value larger than maximal representable value."
        );
        // SAFETY: `fits_within` confirmed the value fits.
        unsafe { Self::__new(expr) }
    }

    /// Returns the wrapped value as the backing type.
    pub const fn get(self) -> T {
        self.0
    }

    /// Increases the number of bits usable for `self`.
    pub fn extend<const M: u32>(self) -> Bounded<T, M> {
        // SAFETY: `N < M` and the value fits in `N` bits, so it fits in `M` bits.
        unsafe { Bounded::<T, M>::__new(self.0) }
    }

    /// Attempts to reduce the number of bits. Returns `None` if the value doesn't fit.
    pub fn try_shrink<const M: u32>(self) -> Option<Bounded<T, M>> {
        Bounded::<T, M>::try_new(self.0)
    }

    /// Changes the backing type. `U` must be at least as wide as `N` bits.
    pub fn cast<U: Integer + From<T>>(self) -> Bounded<U, N> {
        // SAFETY: `U` has at least `N` bits (guaranteed by `From<T>` + `N <= T::BITS`),
        // and the value fits in `N` bits.
        unsafe { Bounded::<U, N>::__new(U::from(self.0)) }
    }
}

// Const `new` for unsigned types
macro_rules! impl_const_new {
    ($($type:ty)*) => {
        $(
        impl<const N: u32> Bounded<$type, N> {
            /// Creates a `Bounded` for the constant `VALUE`.
            /// Fails at build time if `VALUE` cannot be represented with `N` bits.
            pub const fn new<const VALUE: $type>() -> Self {
                let shift: u32 = <$type>::BITS - N;
                assert!((VALUE << shift) >> shift == VALUE);
                // SAFETY: checked above.
                unsafe { Self::__new(VALUE) }
            }
        }
        )*
    };
}

impl_const_new!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize);

// Deref to backing type
impl<T: Integer, const N: u32> ops::Deref for Bounded<T, N> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

// Arithmetic forwarders
impl<T: Integer, const N: u32> ops::Add<T> for Bounded<T, N> {
    type Output = T;
    fn add(self, rhs: T) -> T { self.0 + rhs }
}

impl<T: Integer, const N: u32> ops::Sub<T> for Bounded<T, N> {
    type Output = T;
    fn sub(self, rhs: T) -> T { self.0 - rhs }
}

impl<T: Integer, const N: u32> ops::Mul<T> for Bounded<T, N> {
    type Output = T;
    fn mul(self, rhs: T) -> T { self.0 * rhs }
}

impl<T: Integer, const N: u32> ops::Div<T> for Bounded<T, N> {
    type Output = T;
    fn div(self, rhs: T) -> T { self.0 / rhs }
}

impl<T: Integer, const N: u32> ops::Rem<T> for Bounded<T, N> {
    type Output = T;
    fn rem(self, rhs: T) -> T { self.0 % rhs }
}

// From<bool> for 1-bit Bounded
impl<T: Integer + From<bool>, const N: u32> From<bool> for Bounded<T, N> where
    Bounded<T, N>: Sized
{
    fn from(value: bool) -> Self {
        // SAFETY: `bool` is 0 or 1, which fits in any `N >= 1` bits.
        unsafe { Self::__new(T::from(value)) }
    }
}

// From<smaller unsigned> for Bounded with enough bits
macro_rules! impl_from_unsigned {
    ($src:ty => $dst:ty, $($N:literal),*) => {
        $(
        impl From<$src> for Bounded<$dst, $N> {
            fn from(value: $src) -> Self {
                // SAFETY: $src has at most $N bits.
                unsafe { Self::__new(value as $dst) }
            }
        }
        )*
    };
}

impl_from_unsigned!(u8 => u16, 8);
impl_from_unsigned!(u8 => u32, 8);
impl_from_unsigned!(u8 => u64, 8);
impl_from_unsigned!(u8 => usize, 8);
impl_from_unsigned!(u16 => u32, 16);
impl_from_unsigned!(u16 => u64, 16);
impl_from_unsigned!(u32 => u64, 32);

// ── TryIntoBounded trait ─────────────────────────────────────────────────

/// Fallible conversion from any primitive integer to a `Bounded`.
pub trait TryIntoBounded {
    fn try_into_bounded<T: Integer, const N: u32>(self) -> Option<Bounded<T, N>>
    where
        T: TryFrom<Self>,
        Self: Sized + Copy;
}

macro_rules! impl_try_into_bounded {
    ($($type:ty)*) => {
        $(
        impl TryIntoBounded for $type {
            fn try_into_bounded<T: Integer, const N: u32>(self) -> Option<Bounded<T, N>>
            where
                T: TryFrom<Self>,
            {
                let v: T = T::try_from(self).ok()?;
                Bounded::<T, N>::try_new(v)
            }
        }
        )*
    };
}

impl_try_into_bounded!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize);

// ── bitfield! macro ──────────────────────────────────────────────────────

/// Declares a bitfield struct with typed fields.
///
/// # Syntax
/// ```text
/// bitfield! {
///     pub struct Name(u32) {
///         31:16 field_a;
///         15:0  field_b;
///     }
/// }
/// ```
///
/// Generates getters (`field_a()`, `field_b()`) and setters
/// (`with_field_a(value)`, `try_with_field_a(value)`), plus
/// `FIELD_A_MASK`, `FIELD_A_SHIFT` constants.
#[macro_export]
macro_rules! bitfield {
    (
        $(#[$attr:meta])*
        $vis:vis struct $Name:ident($storage:ty) {
            $(
                $(#[$field_attr:meta])*
                $hi:literal : $lo:literal $field:ident $(=> $conv:ty)? $(?=> $conv_fall:ty)?;
            )+
        }
    ) => {
        $(#[$attr])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        $vis struct $Name($storage);

        impl $Name {
            /// Create a zeroed instance.
            pub const fn zeroed() -> Self {
                Self(0)
            }

            /// Create from raw storage.
            pub const fn from_raw(raw: $storage) -> Self {
                Self(raw)
            }

            /// Convert to raw storage.
            pub const fn into_raw(self) -> $storage {
                self.0
            }

            $(
                // Constants
                /// Field mask.
                pub const [<$field:upper _SHIFT>]: u32 = $lo;
                /// Field shift.
                pub const [<$field:upper _MASK>]: $storage = ((1 as $storage) << ($hi - $lo + 1)) - 1;

                // Getter
                #[inline]
                pub fn $field(self) -> $storage {
                    (self.0 >> $lo) & Self::[<$field:upper _MASK>]
                }

                // Infallible setter (for values known to fit)
                #[inline]
                pub fn [with_ $field](self, value: $storage) -> Self {
                    let masked = value & Self::[<$field:upper _MASK>];
                    Self((self.0 & !(Self::[<$field:upper _MASK>] << $lo)) | (masked << $lo))
                }

                // Fallible setter
                #[inline]
                pub fn [try_with_ $field](self, value: $storage) -> Option<Self> {
                    if value & !Self::[<$field:upper _MASK>] != 0 {
                        return None;
                    }
                    Some(self.[with_ $field](value))
                }
            )+
        }

        impl From<$storage> for $Name {
            fn from(raw: $storage) -> Self {
                Self(raw)
            }
        }

        impl From<$Name> for $storage {
            fn from(val: $Name) -> Self {
                val.0
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bounded_basic() {
        let v = Bounded::<u8, 4>::new::<15>();
        assert_eq!(v.get(), 15);

        assert!(Bounded::<u8, 4>::try_new(15).is_some());
        assert!(Bounded::<u8, 4>::try_new(16).is_none());
    }

    #[test_case]
    fn test_bounded_extend_shrink() {
        let v = Bounded::<u32, 12>::new::<127>();
        let _ = v.extend::<15>();
        assert!(v.try_shrink::<8>().is_some());
        assert!(v.try_shrink::<6>().is_none());
    }

    #[test_case]
    fn test_bitfield() {
        bitfield! {
            struct Rgb(u16) {
                15:11 blue;
                10:5  green;
                4:0   red;
            }
        }

        let color = Rgb::zeroed()
            .with_red(0x10)
            .with_green(0x1f)
            .with_blue(0x18);

        assert_eq!(color.red(), 0x10);
        assert_eq!(color.green(), 0x1f);
        assert_eq!(color.blue(), 0x18);
        assert_eq!(
            color.into_raw(),
            (0x18 << Rgb::BLUE_SHIFT) + (0x1f << Rgb::GREEN_SHIFT) + 0x10,
        );
    }
}
