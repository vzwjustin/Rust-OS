//! Internal module for pin-init.
//!
//! Ported from Linux `rust/pin-init/src/__internal.rs`.

use core::marker::PhantomData;

/// Zero-sized type used to mark a type as invariant.
#[repr(transparent)]
pub struct PhantomInvariant<T: ?Sized>(PhantomData<fn(T) -> T>);

impl<T: ?Sized> Clone for PhantomInvariant<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for PhantomInvariant<T> {}

impl<T: ?Sized> Default for PhantomInvariant<T> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> PhantomInvariant<T> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

/// Zero-sized type used to mark a lifetime as invariant.
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PhantomInvariantLifetime<'a>(PhantomInvariant<&'a ()>);

impl PhantomInvariantLifetime<'_> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(PhantomInvariant::new())
    }
}

/// Type that can only be created by `unsafe` code, used as a token for `PinnedDrop::drop`.
pub struct OnlyCallFromDrop(());

impl OnlyCallFromDrop {
    /// # Safety
    ///
    /// This must only be called from `Drop::drop`.
    #[inline(always)]
    pub unsafe fn new() -> Self {
        Self(())
    }
}

/// Stack initialization helper.
pub struct StackInit<T> {
    inner: core::mem::MaybeUninit<T>,
}

impl<T> StackInit<T> {
    pub fn uninit() -> Self {
        Self {
            inner: core::mem::MaybeUninit::uninit(),
        }
    }

    pub fn init<E>(
        mut this: core::pin::Pin<&mut Self>,
        init: impl super::PinInit<T, E>,
    ) -> Result<core::pin::Pin<&mut T>, E> {
        let slot = this.inner.as_mut_ptr();
        unsafe { init.__pinned_init(slot)? };
        let ref_mut = unsafe { &mut *this.inner.as_mut_ptr() };
        Ok(unsafe { core::pin::Pin::new_unchecked(ref_mut) })
    }
}

/// Marker trait for types that have pin data (used by `assert_pinned!`).
pub trait HasPinData {
    type PinData;
    fn __pin_data() -> Self::PinData;
}

/// Always-failing initializer (used by `assert_pinned!`).
pub struct AlwaysFail<T>(core::marker::PhantomData<T>);

impl<T> AlwaysFail<T> {
    pub fn new() -> Self {
        Self(core::marker::PhantomData)
    }
}
