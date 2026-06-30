// SPDX-License-Identifier: GPL-2.0
//! Pin-init pattern — simplified port of Linux `rust/kernel/init.rs`.
//!
//! This module provides the core `PinInit` / `Init` traits without depending
//! on the upstream `pin-init` crate.  The design closely follows Linux's
//! approach so that future migration to the crate is straightforward.

#![allow(dead_code, unused_variables, unused_imports)]

use core::{
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
};

// ---------------------------------------------------------------------------
// Zeroable
// ---------------------------------------------------------------------------

/// Types whose all-zero bit-pattern is a valid, initialized value.
///
/// # Safety
///
/// Implementing this trait asserts that `core::mem::zeroed::<Self>()` produces
/// a valid, fully initialized instance of `Self`.
pub unsafe trait Zeroable: Sized {}

// Blanket implementations for primitive types.
macro_rules! impl_zeroable {
    ($($ty:ty),* $(,)?) => { $(unsafe impl Zeroable for $ty {})* };
}
impl_zeroable!(
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    f32, f64, bool, char,
);
unsafe impl<T: Zeroable> Zeroable for Option<T> {}

// ---------------------------------------------------------------------------
// Init<T, E> trait
// ---------------------------------------------------------------------------

/// An initializer for a value of type `T` that may fail with error `E`.
///
/// Unlike `PinInit`, the value need not be pinned after initialization.
pub trait Init<T, E = core::convert::Infallible>: Sized {
    /// Initialize the pointed-to slot.
    ///
    /// # Safety
    ///
    /// - `slot` must be a valid, properly aligned, non-null pointer to
    ///   uninitialized memory of size `size_of::<T>()`.
    /// - On `Ok(())` return the pointee is fully initialized.
    /// - On `Err(_)` return the pointee must remain uninitialized.
    unsafe fn __init(self, slot: *mut T) -> Result<(), E>;

    /// Chain this initializer with a closure run after successful initialization.
    fn chain<F>(self, f: F) -> ChainInit<Self, F, T, E>
    where
        F: FnOnce(&mut T) -> Result<(), E>,
    {
        ChainInit { init: self, f, _phantom: PhantomData }
    }
}

/// Adapter returned by [`Init::chain`].
pub struct ChainInit<I, F, T, E> {
    init: I,
    f: F,
    _phantom: PhantomData<fn(T) -> Result<(), E>>,
}

impl<I, F, T, E> Init<T, E> for ChainInit<I, F, T, E>
where
    I: Init<T, E>,
    F: FnOnce(&mut T) -> Result<(), E>,
{
    unsafe fn __init(self, slot: *mut T) -> Result<(), E> {
        // SAFETY: caller contract.
        unsafe { self.init.__init(slot)? };
        // SAFETY: slot was just initialized by `init`.
        let val = unsafe { &mut *slot };
        (self.f)(val)
    }
}

// ---------------------------------------------------------------------------
// PinInit<T, E> trait
// ---------------------------------------------------------------------------

/// An initializer for a pinned value of type `T` that may fail with `E`.
///
/// After `__pinned_init` returns `Ok(())`, the pointee is considered
/// initialized AND pinned — the caller must not move it.
pub trait PinInit<T, E = core::convert::Infallible>: Sized {
    /// Initialize the pointed-to slot in place.
    ///
    /// # Safety
    ///
    /// Same as [`Init::__init`] plus: the caller must not move the value
    /// after this call returns `Ok(())`.
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E>;
}

// Every `Init` is also a `PinInit` (the converse does not hold).
impl<T, E, I: Init<T, E>> PinInit<T, E> for I {
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E> {
        // SAFETY: caller contract.
        unsafe { self.__init(slot) }
    }
}

// ---------------------------------------------------------------------------
// Closure-based constructors
// ---------------------------------------------------------------------------

/// Create an `Init<T, E>` from a closure.
///
/// The closure receives a raw `*mut T` and must initialize it or return an
/// error without leaving the slot partially initialized.
pub fn init_from_closure<T, E>(
    f: impl FnOnce(*mut T) -> Result<(), E>,
) -> impl Init<T, E> {
    struct ClosureInit<F>(F);
    impl<T, E, F: FnOnce(*mut T) -> Result<(), E>> Init<T, E> for ClosureInit<F> {
        unsafe fn __init(self, slot: *mut T) -> Result<(), E> {
            (self.0)(slot)
        }
    }
    ClosureInit(f)
}

/// Create a `PinInit<T, E>` from a closure.
pub fn pin_init_from_closure<T, E>(
    f: impl FnOnce(*mut T) -> Result<(), E>,
) -> impl PinInit<T, E> {
    init_from_closure(f)
}

// ---------------------------------------------------------------------------
// zeroed() — zero-initialize any Zeroable type
// ---------------------------------------------------------------------------

/// Returns an `Init<T>` that zero-initializes `T`.
pub fn zeroed<T: Zeroable>() -> impl Init<T> {
    init_from_closure(|slot: *mut T| {
        // SAFETY: T: Zeroable guarantees that the all-zeros pattern is valid.
        unsafe { slot.write_bytes(0, 1) };
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// InPlaceInit — helper for Box / Pin<Box<T>>
// ---------------------------------------------------------------------------

/// Extension trait providing in-place initialization for smart pointers.
pub trait InPlaceInit<T>: Sized {
    /// Try to create a `Pin<Self>` using a `PinInit`.
    fn try_pin_init<E>(init: impl PinInit<T, E>) -> Result<Pin<Self>, E>
    where
        Self: core::ops::DerefMut<Target = T>,
    ;

    /// Try to create a `Self` using an `Init`.
    fn try_init<E>(init: impl Init<T, E>) -> Result<Self, E>
    where
        Self: core::ops::DerefMut<Target = T>,
    ;
}

impl<T> InPlaceInit<T> for alloc::boxed::Box<T> {
    fn try_pin_init<E>(init: impl PinInit<T, E>) -> Result<Pin<Self>, E>
    where
        Self: core::ops::DerefMut<Target = T>,
    {
        let mut boxed = alloc::boxed::Box::new(MaybeUninit::<T>::uninit());
        // SAFETY: `boxed` is heap-allocated; we're about to initialize it.
        unsafe { init.__pinned_init(boxed.as_mut_ptr())? };
        // SAFETY: the value is initialized and heap-allocated (stable address).
        Ok(unsafe { Pin::new_unchecked(alloc::boxed::Box::from_raw(alloc::boxed::Box::into_raw(boxed) as *mut T)) })
    }

    fn try_init<E>(init: impl Init<T, E>) -> Result<Self, E>
    where
        Self: core::ops::DerefMut<Target = T>,
    {
        let mut boxed = alloc::boxed::Box::new(MaybeUninit::<T>::uninit());
        // SAFETY: `boxed` is heap-allocated.
        unsafe { init.__init(boxed.as_mut_ptr())? };
        // SAFETY: initialized above.
        Ok(unsafe { alloc::boxed::Box::from_raw(alloc::boxed::Box::into_raw(boxed) as *mut T) })
    }
}

// ---------------------------------------------------------------------------
// Convenience macros
// ---------------------------------------------------------------------------

/// Initialize a struct in place, pinned.
///
/// Mirrors the spirit of `pin_init!` from the `pin-init` crate but works
/// purely in terms of field-by-field `Init` / `PinInit` impls.
///
/// This macro is intentionally minimal.  For complex structs with
/// self-referential fields, use `pin_init_from_closure` directly.
#[macro_export]
macro_rules! pin_init {
    ($ty:ty { $($field:ident: $init:expr),* $(,)? }) => {
        $crate::init::init_from_closure(|slot: *mut $ty| {
            // SAFETY: We write every field; Rust guarantees the struct is
            // fully initialized if all fields are written.
            unsafe {
                $(
                    ::core::ptr::addr_of_mut!((*slot).$field).write($init);
                )*
            }
            Ok(())
        })
    };
}

/// Fallible version of `pin_init!`.
#[macro_export]
macro_rules! try_pin_init {
    ($ty:ty { $($field:ident: $init:expr),* $(,)? } ? $err:ty) => {
        $crate::init::init_from_closure(|slot: *mut $ty| -> Result<(), $err> {
            unsafe {
                $(
                    ::core::ptr::addr_of_mut!((*slot).$field).write($init);
                )*
            }
            Ok(())
        })
    };
}
