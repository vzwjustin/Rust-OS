//! Pin-initialization library.
//!
//! Ported from Linux `rust/pin-init/src/lib.rs`.
//!
//! Provides `PinInit<T, E>` and `Init<T, E>` traits for safe in-place
//! initialization of pinned structs, plus `Zeroable` trait and helpers.
//!
//! The proc-macro-dependent parts (`#[pin_data]`, `pin_init!`, `init!`)
//! are replaced with declarative macro approximations.

#![allow(clippy::missing_safety_doc)]

use core::{
    cell::UnsafeCell,
    convert::Infallible,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    ptr::{self, NonNull},
};

pub mod __internal;

// ── Core traits ──────────────────────────────────────────────────────────

/// A pin-initializer for the type `T`.
///
/// # Safety
///
/// `__pinned_init`:
/// - returns `Ok(())` if it initialized every field of `slot`,
/// - returns `Err(err)` if it encountered an error and then cleaned `slot`,
/// - while constructing the `T` at `slot` it upholds the pinning invariants of `T`.
#[must_use = "An initializer must be used in order to create its value."]
pub unsafe trait PinInit<T: ?Sized, E = Infallible>: Sized {
    /// Initializes `slot`.
    ///
    /// # Safety
    ///
    /// - `slot` is a valid pointer to uninitialized memory.
    /// - the caller does not touch `slot` when `Err` is returned.
    /// - `slot` will not move until it is dropped, i.e. it will be pinned.
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E>;

    /// First initializes the value using `self` then calls `f` with the initialized value.
    fn pin_chain<F>(self, f: F) -> ChainPinInit<Self, F, T, E>
    where
        F: FnOnce(Pin<&mut T>) -> Result<(), E>,
    {
        ChainPinInit(self, f, __internal::PhantomInvariant::new())
    }
}

/// An initializer returned by [`PinInit::pin_chain`].
pub struct ChainPinInit<I, F, T: ?Sized, E>(I, F, __internal::PhantomInvariant<(E, T)>);

unsafe impl<T: ?Sized, E, I, F> PinInit<T, E> for ChainPinInit<I, F, T, E>
where
    I: PinInit<T, E>,
    F: FnOnce(Pin<&mut T>) -> Result<(), E>,
{
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E> {
        unsafe { self.0.__pinned_init(slot)? };
        let val = unsafe { &mut *slot };
        let val = unsafe { Pin::new_unchecked(val) };
        (self.1)(val).inspect_err(|_| unsafe { core::ptr::drop_in_place(slot) })
    }
}

/// An initializer for `T`.
///
/// # Safety
///
/// Same as `PinInit`, but the pointee may be moved after initialization.
#[must_use = "An initializer must be used in order to create its value."]
pub unsafe trait Init<T: ?Sized, E = Infallible>: PinInit<T, E> {
    /// Initializes `slot`.
    ///
    /// # Safety
    ///
    /// - `slot` is a valid pointer to uninitialized memory.
    /// - the caller does not touch `slot` when `Err` is returned.
    unsafe fn __init(self, slot: *mut T) -> Result<(), E>;

    /// First initializes the value using `self` then calls `f` with the initialized value.
    fn chain<F>(self, f: F) -> ChainInit<Self, F, T, E>
    where
        F: FnOnce(&mut T) -> Result<(), E>,
    {
        ChainInit(self, f, __internal::PhantomInvariant::new())
    }
}

/// An initializer returned by [`Init::chain`].
pub struct ChainInit<I, F, T: ?Sized, E>(I, F, __internal::PhantomInvariant<(E, T)>);

unsafe impl<T: ?Sized, E, I, F> Init<T, E> for ChainInit<I, F, T, E>
where
    I: Init<T, E>,
    F: FnOnce(&mut T) -> Result<(), E>,
{
    unsafe fn __init(self, slot: *mut T) -> Result<(), E> {
        unsafe { self.0.__pinned_init(slot)? };
        (self.1)(unsafe { &mut *slot })
            .inspect_err(|_| unsafe { core::ptr::drop_in_place(slot) })
    }
}

unsafe impl<T: ?Sized, E, I, F> PinInit<T, E> for ChainInit<I, F, T, E>
where
    I: Init<T, E>,
    F: FnOnce(&mut T) -> Result<(), E>,
{
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E> {
        unsafe { self.__init(slot) }
    }
}

// ── Closure-based init ───────────────────────────────────────────────────

struct InitClosure<F, T: ?Sized>(F, __internal::PhantomInvariant<T>);

unsafe impl<T: ?Sized, F, E> Init<T, E> for InitClosure<F, T>
where
    F: FnOnce(*mut T) -> Result<(), E>,
{
    #[inline]
    unsafe fn __init(self, slot: *mut T) -> Result<(), E> {
        (self.0)(slot)
    }
}

unsafe impl<T: ?Sized, F, E> PinInit<T, E> for InitClosure<F, T>
where
    F: FnOnce(*mut T) -> Result<(), E>,
{
    #[inline]
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E> {
        (self.0)(slot)
    }
}

/// Creates a new `PinInit<T, E>` from the given closure.
///
/// # Safety
///
/// The closure must fulfill the `__pinned_init` safety requirements.
#[inline]
pub const unsafe fn pin_init_from_closure<T: ?Sized, E>(
    f: impl FnOnce(*mut T) -> Result<(), E>,
) -> impl PinInit<T, E> {
    InitClosure(f, __internal::PhantomInvariant::new())
}

/// Creates a new `Init<T, E>` from the given closure.
///
/// # Safety
///
/// The closure must fulfill the `__init` safety requirements.
#[inline]
pub const unsafe fn init_from_closure<T: ?Sized, E>(
    f: impl FnOnce(*mut T) -> Result<(), E>,
) -> impl Init<T, E> {
    InitClosure(f, __internal::PhantomInvariant::new())
}

/// Changes the type to be initialized.
///
/// # Safety
///
/// `*mut U` must be castable to `*mut T`.
pub const unsafe fn cast_pin_init<T, U, E>(init: impl PinInit<T, E>) -> impl PinInit<U, E> {
    unsafe { pin_init_from_closure(|ptr: *mut U| init.__pinned_init(ptr.cast::<T>())) }
}

/// Changes the type to be initialized.
///
/// # Safety
///
/// `*mut U` must be castable to `*mut T`.
pub const unsafe fn cast_init<T, U, E>(init: impl Init<T, E>) -> impl Init<U, E> {
    unsafe { init_from_closure(|ptr: *mut U| init.__init(ptr.cast::<T>())) }
}

// ── Blanket impls ────────────────────────────────────────────────────────

unsafe impl<T> Init<T> for T {
    unsafe fn __init(self, slot: *mut T) -> Result<(), Infallible> {
        unsafe { slot.write(self) };
        Ok(())
    }
}

unsafe impl<T> PinInit<T> for T {
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), Infallible> {
        unsafe { slot.write(self) };
        Ok(())
    }
}

unsafe impl<T, E> Init<T, E> for Result<T, E> {
    unsafe fn __init(self, slot: *mut T) -> Result<(), E> {
        unsafe { slot.write(self?) };
        Ok(())
    }
}

unsafe impl<T, E> PinInit<T, E> for Result<T, E> {
    unsafe fn __pinned_init(self, slot: *mut T) -> Result<(), E> {
        unsafe { slot.write(self?) };
        Ok(())
    }
}

// ── InPlaceWrite trait ───────────────────────────────────────────────────

/// Smart pointer containing uninitialized memory that can write a value.
pub trait InPlaceWrite<T> {
    type Initialized;

    fn write_init<E>(self, init: impl Init<T, E>) -> Result<Self::Initialized, E>;
    fn write_pin_init<E>(self, init: impl PinInit<T, E>) -> Result<Pin<Self::Initialized>, E>;
}

impl<T> InPlaceWrite<T> for &'static mut MaybeUninit<T> {
    type Initialized = &'static mut T;

    fn write_init<E>(self, init: impl Init<T, E>) -> Result<Self::Initialized, E> {
        let slot = self.as_mut_ptr();
        unsafe { init.__init(slot)? };
        unsafe { Ok(self.assume_init_mut()) }
    }

    fn write_pin_init<E>(self, init: impl PinInit<T, E>) -> Result<Pin<Self::Initialized>, E> {
        let slot = self.as_mut_ptr();
        unsafe { init.__pinned_init(slot)? };
        Ok(Pin::static_mut(unsafe { self.assume_init_mut() }))
    }
}

// ── InPlaceInit trait (alloc feature) ────────────────────────────────────

pub extern crate alloc;

use alloc::{boxed::Box, sync::Arc};

/// Smart pointer that can initialize memory in-place.
pub trait InPlaceInit<T>: Sized {
    fn try_pin_init<E>(init: impl PinInit<T, E>) -> Result<Pin<Self>, E>
    where
        E: From<AllocError>;

    fn pin_init(init: impl PinInit<T>) -> Result<Pin<Self>, AllocError> {
        let init = unsafe {
            pin_init_from_closure(|slot| match init.__pinned_init(slot) {
                Ok(()) => Ok(()),
                Err(i) => match i {},
            })
        };
        Self::try_pin_init(init)
    }

    fn try_init<E>(init: impl Init<T, E>) -> Result<Self, E>
    where
        E: From<AllocError>;

    fn init(init: impl Init<T>) -> Result<Self, AllocError> {
        let init = unsafe {
            init_from_closure(|slot| match init.__init(slot) {
                Ok(()) => Ok(()),
                Err(i) => match i {},
            })
        };
        Self::try_init(init)
    }
}

impl<T> InPlaceInit<T> for Box<T> {
    #[inline]
    fn try_pin_init<E>(init: impl PinInit<T, E>) -> Result<Pin<Self>, E>
    where
        E: From<AllocError>,
    {
        Box::try_new_uninit()?.write_pin_init(init)
    }

    #[inline]
    fn try_init<E>(init: impl Init<T, E>) -> Result<Self, E>
    where
        E: From<AllocError>,
    {
        Box::try_new_uninit()?.write_init(init)
    }
}

impl<T> InPlaceInit<T> for Arc<T> {
    #[inline]
    fn try_pin_init<E>(init: impl PinInit<T, E>) -> Result<Pin<Self>, E>
    where
        E: From<AllocError>,
    {
        let mut this = Arc::try_new_uninit()?;
        let Some(slot) = Arc::get_mut(&mut this) else {
            unsafe { core::hint::unreachable_unchecked() }
        };
        let slot = slot.as_mut_ptr();
        unsafe { init.__pinned_init(slot)? };
        Ok(unsafe { Pin::new_unchecked(this.assume_init()) })
    }

    #[inline]
    fn try_init<E>(init: impl Init<T, E>) -> Result<Self, E>
    where
        E: From<AllocError>,
    {
        let mut this = Arc::try_new_uninit()?;
        let Some(slot) = Arc::get_mut(&mut this) else {
            unsafe { core::hint::unreachable_unchecked() }
        };
        let slot = slot.as_mut_ptr();
        unsafe { init.__init(slot)? };
        Ok(unsafe { this.assume_init() })
    }
}

impl<T> InPlaceWrite<T> for Box<MaybeUninit<T>> {
    type Initialized = Box<T>;

    fn write_init<E>(mut self, init: impl Init<T, E>) -> Result<Self::Initialized, E> {
        let slot = self.as_mut_ptr();
        unsafe { init.__init(slot)? };
        Ok(unsafe { self.assume_init() })
    }

    fn write_pin_init<E>(mut self, init: impl PinInit<T, E>) -> Result<Pin<Self::Initialized>, E> {
        let slot = self.as_mut_ptr();
        unsafe { init.__pinned_init(slot)? };
        Ok(unsafe { self.assume_init() }.into())
    }
}

// ── Zeroable trait ───────────────────────────────────────────────────────

/// Marker trait for types that can be initialized by writing just zeroes.
///
/// # Safety
///
/// The bit pattern consisting of only zeroes is a valid bit pattern for this type.
pub unsafe trait Zeroable {
    fn init_zeroed() -> impl Init<Self>
    where
        Self: Sized,
    {
        init_zeroed()
    }

    fn zeroed() -> Self
    where
        Self: Sized,
    {
        zeroed()
    }
}

/// Create an initializer for a zeroed `T`.
#[inline]
pub fn init_zeroed<T: Zeroable>() -> impl Init<T> {
    unsafe {
        init_from_closure(|slot: *mut T| {
            slot.write_bytes(0, 1);
            Ok(())
        })
    }
}

/// Create a `T` consisting of all zeroes.
pub const fn zeroed<T: Zeroable>() -> T {
    // SAFETY: The `Zeroable` trait guarantees that an all-zero bit pattern
    // is a valid value of `T`.
    unsafe { core::mem::zeroed() }
}

macro_rules! impl_zeroable {
    ($($($generics:tt)? $t:ty, )*) => {
        $(unsafe impl$($generics)* Zeroable for $t {})*
    };
}

impl_zeroable! {
    bool,
    char,
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    f32, f64,
    {<T: ?Sized>} PhantomData<T>,
    core::marker::PhantomPinned,
    (),
    {<T>} MaybeUninit<T>,
    {<T: ?Sized + Zeroable>} UnsafeCell<T>,
    {<T>} *mut T, {<T>} *const T,
    {<T>} *mut [T], {<T>} *const [T], *mut str, *const str,
    {<const N: usize, T: Zeroable>} [T; N],
    {<T: Zeroable>} core::num::Wrapping<T>,
}

macro_rules! impl_tuple_zeroable {
    ($first:ident, $(,)?) => {
        unsafe impl<$first: Zeroable> Zeroable for ($first,) {}
    };
    ($first:ident, $($t:ident),* $(,)?) => {
        unsafe impl<$first: Zeroable, $($t: Zeroable),*> Zeroable for ($first, $($t),*) {}
        impl_tuple_zeroable!($($t),* ,);
    }
}

impl_tuple_zeroable!(A, B, C, D, E, F, G, H, I, J);

/// Marker trait for types that allow `Option<Self>` to be set to all zeroes.
pub unsafe trait ZeroableOption {}

unsafe impl<T: ZeroableOption> Zeroable for Option<T> {}

macro_rules! impl_zeroable_option {
    ($($($generics:tt)? $t:ty, )*) => {
        $(unsafe impl$($generics)* ZeroableOption for $t {})*
    };
}

impl_zeroable_option! {
    {<T: ?Sized>} &T,
    {<T: ?Sized>} &mut T,
    {<T: ?Sized>} NonNull<T>,
    core::num::NonZero<u8>, core::num::NonZero<u16>, core::num::NonZero<u32>,
    core::num::NonZero<u64>, core::num::NonZero<u128>, core::num::NonZero<usize>,
    core::num::NonZero<i8>, core::num::NonZero<i16>, core::num::NonZero<i32>,
    core::num::NonZero<i64>, core::num::NonZero<i128>, core::num::NonZero<isize>,
}

// SAFETY: All zeros is equivalent to `None`.
unsafe impl<T> ZeroableOption for Box<T> {}

// ── Wrapper trait ────────────────────────────────────────────────────────

/// Allows creating an instance of `Self` containing exactly one structurally pinned value.
pub trait Wrapper<T> {
    fn pin_init<E>(value_init: impl PinInit<T, E>) -> impl PinInit<Self, E>;
}

impl<T> Wrapper<T> for UnsafeCell<T> {
    fn pin_init<E>(value_init: impl PinInit<T, E>) -> impl PinInit<Self, E> {
        unsafe { cast_pin_init(value_init) }
    }
}

impl<T> Wrapper<T> for MaybeUninit<T> {
    fn pin_init<E>(value_init: impl PinInit<T, E>) -> impl PinInit<Self, E> {
        unsafe { cast_pin_init(value_init) }
    }
}

// ── PinnedDrop trait ─────────────────────────────────────────────────────

/// Trait facilitating pinned destruction.
///
/// # Safety
///
/// Must be implemented via the `#[pinned_drop]` proc-macro attribute.
pub unsafe trait PinnedDrop {
    fn drop(self: Pin<&mut Self>, only_call_from_drop: __internal::OnlyCallFromDrop);
}

// ── Utility functions ────────────────────────────────────────────────────

/// An initializer that leaves the memory uninitialized.
#[inline]
pub fn uninit<T, E>() -> impl Init<MaybeUninit<T>, E> {
    unsafe { init_from_closure(|_| Ok(())) }
}

/// Initializes an array by initializing each element via the provided initializer.
pub fn init_array_from_fn<I, const N: usize, T, E>(
    mut make_init: impl FnMut(usize) -> I,
) -> impl Init<[T; N], E>
where
    I: Init<T, E>,
{
    let init = move |slot: *mut [T; N]| {
        let slot = slot.cast::<T>();
        for i in 0..N {
            let init = make_init(i);
            let ptr = unsafe { slot.add(i) };
            if let Err(e) = unsafe { init.__init(ptr) } {
                // SAFETY: `slot` is a valid `*mut T` array and `i` is the
                // number of initialized elements to drop.
                unsafe { ptr::drop_in_place(ptr::slice_from_raw_parts_mut(slot, i)) };
                return Err(e);
            }
        }
        Ok(())
    };
    unsafe { init_from_closure(init) }
}

/// Initializes an array by pin-initializing each element.
pub fn pin_init_array_from_fn<I, const N: usize, T, E>(
    mut make_init: impl FnMut(usize) -> I,
) -> impl PinInit<[T; N], E>
where
    I: PinInit<T, E>,
{
    let init = move |slot: *mut [T; N]| {
        let slot = slot.cast::<T>();
        for i in 0..N {
            let init = make_init(i);
            let ptr = unsafe { slot.add(i) };
            if let Err(e) = unsafe { init.__pinned_init(ptr) } {
                // SAFETY: `slot` is a valid `*mut T` array and `i` is the
                // number of initialized elements to drop.
                unsafe { ptr::drop_in_place(ptr::slice_from_raw_parts_mut(slot, i)) };
                return Err(e);
            }
        }
        Ok(())
    };
    unsafe { pin_init_from_closure(init) }
}

/// Construct an initializer in a closure and run it.
pub fn pin_init_scope<T, E, F, I>(make_init: F) -> impl PinInit<T, E>
where
    F: FnOnce() -> Result<I, E>,
    I: PinInit<T, E>,
{
    unsafe {
        pin_init_from_closure(move |slot: *mut T| -> Result<(), E> {
            let init = make_init()?;
            init.__pinned_init(slot)
        })
    }
}

/// Construct an initializer in a closure and run it.
pub fn init_scope<T, E, F, I>(make_init: F) -> impl Init<T, E>
where
    F: FnOnce() -> Result<I, E>,
    I: Init<T, E>,
{
    unsafe {
        init_from_closure(move |slot: *mut T| -> Result<(), E> {
            let init = make_init()?;
            init.__init(slot)
        })
    }
}

// ── AllocError ───────────────────────────────────────────────────────────

/// Error type for allocation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocError;

impl From<AllocError> for AllocError {
    fn from(e: AllocError) -> Self { e }
}

// ── Stack init helpers ───────────────────────────────────────────────────

/// Initialize and pin a type directly on the stack.
#[macro_export]
macro_rules! stack_pin_init {
    (let $var:ident $(: $t:ty)? = $val:expr) => {
        let val = $val;
        let mut $var = ::core::pin::pin!($crate::linux_rust::pin_init::__internal::StackInit$(::<$t>)?::uninit());
        let mut $var = match $crate::linux_rust::pin_init::__internal::StackInit::init($var, val) {
            Ok(res) => res,
            Err(x) => {
                let x: ::core::convert::Infallible = x;
                match x {}
            }
        };
    };
}

/// Initialize and pin a type directly on the stack (fallible).
#[macro_export]
macro_rules! stack_try_pin_init {
    (let $var:ident $(: $t:ty)? = $val:expr) => {
        let val = $val;
        let mut $var = ::core::pin::pin!($crate::linux_rust::pin_init::__internal::StackInit$(::<$t>)?::uninit());
        let mut $var = $crate::linux_rust::pin_init::__internal::StackInit::init($var, val);
    };
    (let $var:ident $(: $t:ty)? =? $val:expr) => {
        let val = $val;
        let mut $var = ::core::pin::pin!($crate::linux_rust::pin_init::__internal::StackInit$(::<$t>)?::uninit());
        let mut $var = $crate::linux_rust::pin_init::__internal::StackInit::init($var, val)?;
    };
}
