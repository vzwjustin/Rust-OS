// SPDX-License-Identifier: GPL-2.0
//! Core kernel type abstractions — ported from Linux `rust/kernel/types.rs`.
//!
//! All `bindings::` calls removed; this is pure-Rust / no_std.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    pin::Pin,
};

// ---------------------------------------------------------------------------
// Either<L, R>
// ---------------------------------------------------------------------------

/// A value that is either `Left(L)` or `Right(R)`.
pub enum Either<L, R> {
    /// Left variant.
    Left(L),
    /// Right variant.
    Right(R),
}

impl<L, R> Either<L, R> {
    /// Returns `true` if this is the `Left` variant.
    pub fn is_left(&self) -> bool {
        matches!(self, Either::Left(_))
    }

    /// Returns `true` if this is the `Right` variant.
    pub fn is_right(&self) -> bool {
        matches!(self, Either::Right(_))
    }
}

// ---------------------------------------------------------------------------
// NotThreadSafe
// ---------------------------------------------------------------------------

/// A zero-sized marker that makes the containing type `!Send + !Sync`.
///
/// This is identical to Linux's `NotThreadSafe` which is `PhantomData<*mut ()>`.
pub type NotThreadSafe = PhantomData<*mut ()>;

// ---------------------------------------------------------------------------
// ScopeGuard<T, F>
// ---------------------------------------------------------------------------

/// RAII scope guard — runs a closure when dropped, unless dismissed.
///
/// Derefs to the guarded data `T` (if any).
///
/// # Invariant
///
/// The `Option` is `Some` from construction until either `drop` or `dismiss`
/// consumes it.
pub struct ScopeGuard<T, F: FnOnce(T)>(Option<(T, F)>);

impl<T, F: FnOnce(T)> ScopeGuard<T, F> {
    /// Create a new guard with data `data` and cleanup closure `cleanup`.
    #[inline]
    pub fn new_with_data(data: T, cleanup: F) -> Self {
        ScopeGuard(Some((data, cleanup)))
    }

    /// Dismiss the guard so that the cleanup closure is **not** called on drop.
    ///
    /// Returns the guarded data.
    #[inline]
    pub fn dismiss(mut self) -> T {
        // INVARIANT: after this the `Option` is `None`.
        self.0.take().unwrap().0
    }
}

impl<F: FnOnce(())> ScopeGuard<(), F> {
    /// Create a guard that holds no data (the common case for "run on scope exit").
    #[inline]
    pub fn new(cleanup: F) -> Self {
        Self::new_with_data((), cleanup)
    }
}

impl<T, F: FnOnce(T)> Deref for ScopeGuard<T, F> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // INVARIANT: `0` is `Some` while the guard is alive.
        &self.0.as_ref().unwrap().0
    }
}

impl<T, F: FnOnce(T)> DerefMut for ScopeGuard<T, F> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0.as_mut().unwrap().0
    }
}

impl<T, F: FnOnce(T)> Drop for ScopeGuard<T, F> {
    fn drop(&mut self) {
        if let Some((data, cleanup)) = self.0.take() {
            cleanup(data);
        }
    }
}

// ---------------------------------------------------------------------------
// Opaque<T>
// ---------------------------------------------------------------------------

/// Wraps a value of type `T` in a way that is safe to use with C-style
/// in-place initialization.
///
/// The value is held inside an `UnsafeCell<MaybeUninit<T>>` so that:
/// - The Rust compiler does not assume it is initialized.
/// - Interior mutability is permitted (C code may mutate it).
/// - The address is stable (via `Pin`).
///
/// Use [`Opaque::uninit`] to allocate and [`Opaque::get`] to obtain a raw
/// pointer for passing to init functions.
pub struct Opaque<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    _pin: core::marker::PhantomPinned,
}

impl<T> Opaque<T> {
    /// Create a new `Opaque` with the given `val`.
    pub fn new(val: T) -> Self {
        Opaque {
            value: UnsafeCell::new(MaybeUninit::new(val)),
            _pin: core::marker::PhantomPinned,
        }
    }

    /// Create a new `Opaque` whose inner value is uninitialized.
    ///
    /// The caller must initialize the value before calling [`Opaque::get`] for
    /// reading.
    pub const fn uninit() -> Self {
        Opaque {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            _pin: core::marker::PhantomPinned,
        }
    }

    /// Create a new `Opaque` whose inner value is zeroed.
    ///
    /// Valid only when `T: Zeroable` (caller's responsibility to verify).
    pub fn zeroed() -> Self {
        Opaque {
            value: UnsafeCell::new(MaybeUninit::zeroed()),
            _pin: core::marker::PhantomPinned,
        }
    }

    /// Returns a raw pointer to the wrapped value.
    ///
    /// The pointer is valid for the lifetime of `&self`.
    #[inline]
    pub fn get(&self) -> *mut T {
        UnsafeCell::get(&self.value).cast::<T>()
    }

    /// Returns a raw pointer from a shared reference to a pinned `Opaque`.
    ///
    /// # Safety
    ///
    /// Same aliasing rules as `UnsafeCell::raw_get`.
    #[inline]
    pub unsafe fn raw_get(this: *const Self) -> *mut T {
        // SAFETY: `Opaque` is `repr(transparent)` up to the `UnsafeCell` nesting.
        unsafe {
            UnsafeCell::raw_get(
                (this as *const UnsafeCell<MaybeUninit<T>>)
            )
            .cast::<T>()
        }
    }
}

// SAFETY: The inner value is wrapped in `UnsafeCell`; sharing across threads
// is governed by the same rules as `UnsafeCell<T>`.
unsafe impl<T: Send> Send for Opaque<T> {}
unsafe impl<T: Sync> Sync for Opaque<T> {}

// ---------------------------------------------------------------------------
// AlwaysRefCounted + ARef<T>
// ---------------------------------------------------------------------------

/// A type whose reference count is always managed internally.
///
/// # Safety
///
/// - `inc_ref` must increment the reference count atomically.
/// - `dec_ref` must decrement the reference count atomically and return
///   `true` when the count reaches zero (indicating the value should be freed).
pub unsafe trait AlwaysRefCounted {
    /// Increment the reference count.
    fn inc_ref(&self);

    /// Decrement the reference count.  Returns `true` when the object should
    /// be freed (count reached zero).
    ///
    /// # Safety
    ///
    /// Callers must ensure the reference count is at least 1 before calling.
    unsafe fn dec_ref(obj: core::ptr::NonNull<Self>) -> bool;
}

/// An owned smart pointer for [`AlwaysRefCounted`] types.
///
/// Cloning increments the reference count; dropping decrements it.
pub struct ARef<T: AlwaysRefCounted> {
    ptr: core::ptr::NonNull<T>,
    _phantom: PhantomData<T>,
}

impl<T: AlwaysRefCounted> ARef<T> {
    /// Construct an `ARef` from a raw non-null pointer.
    ///
    /// # Safety
    ///
    /// - `ptr` must point to a live, valid `T`.
    /// - The reference count of `*ptr` must have been incremented on behalf of
    ///   this `ARef` before calling.
    pub unsafe fn from_raw(ptr: core::ptr::NonNull<T>) -> Self {
        ARef { ptr, _phantom: PhantomData }
    }

    /// Returns a raw pointer to the wrapped value (does not consume the `ARef`).
    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: AlwaysRefCounted> Clone for ARef<T> {
    fn clone(&self) -> Self {
        // Increment the reference count before creating the new `ARef`.
        // SAFETY: `self.ptr` is a valid live reference.
        unsafe { (*self.ptr.as_ptr()).inc_ref() };
        ARef { ptr: self.ptr, _phantom: PhantomData }
    }
}

impl<T: AlwaysRefCounted> Drop for ARef<T> {
    fn drop(&mut self) {
        // SAFETY: reference count is at least 1 because we own a reference.
        unsafe { T::dec_ref(self.ptr); }
    }
}

impl<T: AlwaysRefCounted> Deref for ARef<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the pointer is valid and we hold a reference.
        unsafe { self.ptr.as_ref() }
    }
}

// SAFETY: same rules as Arc<T>.
unsafe impl<T: AlwaysRefCounted + Send + Sync> Send for ARef<T> {}
unsafe impl<T: AlwaysRefCounted + Send + Sync> Sync for ARef<T> {}

// ---------------------------------------------------------------------------
// ForeignOwnable
// ---------------------------------------------------------------------------

/// A type that can transfer ownership across an FFI (or other unsafe)
/// boundary and be reclaimed afterwards.
///
/// This mirrors Linux's `ForeignOwnable` trait.  Instead of `*mut c_void`
/// we use `*mut ()` to avoid a dependency on `libc` types.
pub trait ForeignOwnable: Sized {
    /// The minimum byte alignment of the foreign-owned pointer.  Must be a
    /// power of two ≥ 1.
    const FOREIGN_ALIGN: usize = 1;

    /// Type returned by [`borrow`](Self::borrow).
    type Borrowed<'a>: 'a where Self: 'a;

    /// Type returned by [`borrow_mut`](Self::borrow_mut).
    type BorrowedMut<'a>: 'a where Self: 'a;

    /// Convert `self` into a raw pointer that can cross an FFI boundary.
    ///
    /// The caller takes ownership; the value must eventually be reclaimed
    /// via [`from_foreign`](Self::from_foreign).
    fn into_foreign(self) -> *const ();

    /// Reconstruct a `Self` from a raw pointer created by
    /// [`into_foreign`](Self::into_foreign).
    ///
    /// # Safety
    ///
    /// `ptr` must have been produced by `into_foreign` and not yet reclaimed.
    unsafe fn from_foreign(ptr: *const ()) -> Self;

    /// Borrow the value from a raw pointer without taking ownership.
    ///
    /// # Safety
    ///
    /// `ptr` must have been produced by `into_foreign` and the foreign owner
    /// must keep it alive for `'a`.
    unsafe fn borrow<'a>(ptr: *const ()) -> Self::Borrowed<'a>;

    /// Mutably borrow the value from a raw pointer without taking ownership.
    ///
    /// # Safety
    ///
    /// Same as [`borrow`](Self::borrow), plus no other borrows may be active.
    unsafe fn borrow_mut<'a>(ptr: *const ()) -> Self::BorrowedMut<'a>;

    /// Try to reconstruct a `Self` from a possibly-null pointer.
    ///
    /// Returns `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// If non-null, `ptr` must satisfy the same contract as
    /// [`from_foreign`](Self::from_foreign).
    unsafe fn try_from_foreign(ptr: *const ()) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            // SAFETY: caller guarantees ptr is valid when non-null.
            Some(unsafe { Self::from_foreign(ptr) })
        }
    }
}

// ---------------------------------------------------------------------------
// Box<T>: ForeignOwnable
// ---------------------------------------------------------------------------

impl<T: 'static> ForeignOwnable for alloc::boxed::Box<T> {
    type Borrowed<'a> = &'a T;
    type BorrowedMut<'a> = &'a mut T;

    fn into_foreign(self) -> *const () {
        alloc::boxed::Box::into_raw(self) as *const ()
    }

    unsafe fn from_foreign(ptr: *const ()) -> Self {
        // SAFETY: caller contract.
        unsafe { alloc::boxed::Box::from_raw(ptr as *mut T) }
    }

    unsafe fn borrow<'a>(ptr: *const ()) -> &'a T {
        // SAFETY: caller contract.
        unsafe { &*(ptr as *const T) }
    }

    unsafe fn borrow_mut<'a>(ptr: *const ()) -> &'a mut T {
        // SAFETY: caller contract.
        unsafe { &mut *(ptr as *mut T) }
    }
}
