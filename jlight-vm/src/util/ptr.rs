use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(transparent)]
pub struct Ptr<T: ?Sized>(pub(crate) *mut T);

impl<T: ?Sized> Ptr<T> {
    pub fn get(&self) -> &mut T {
        unsafe { &mut *self.0 }
    }
}

impl<T> Ptr<T> {
    pub fn new(x: T) -> Self {
        Self(Box::into_raw(Box::new(x)))
    }

    pub fn from_ref(x: &T) -> Self {
        Self(x as *const T as *mut T)
    }

    pub fn from_box(b: Box<T>) -> Self {
        Self(Box::into_raw(b))
    }

    pub fn from_pointer(x: *mut T) -> Self {
        Self(x)
    }

    pub fn set(&self, val: T) {
        unsafe { self.0.write(val) };
    }

    pub fn replace(&self, val: T) -> T {
        std::mem::replace(self.get(), val)
    }

    pub fn take(&self) -> T
    where
        T: Default,
    {
        self.replace(T::default())
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    pub fn null() -> Self {
        Self(std::ptr::null_mut())
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]
    pub fn compare_and_swap(&self, current: *mut T, other: *mut T) -> bool {
        self.as_atomic()
            .compare_and_swap(current, other, Ordering::AcqRel)
            == current
    }

    /// Atomically replaces the current pointer with the given one.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]
    pub fn atomic_store(&self, other: *mut T) {
        self.as_atomic().store(other, Ordering::Release);
    }

    /// Atomically loads the pointer.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]
    pub fn atomic_load(&self) -> Self {
        Self(self.as_atomic().load(Ordering::Acquire))
    }

    fn as_atomic(&self) -> &AtomicPtr<T> {
        unsafe { &*(self as *const Ptr<T> as *const AtomicPtr<T>) }
    }
}

use std::hash::*;

impl<T> Hash for Ptr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T> PartialEq for Ptr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Ptr<T> {}

impl<T> Copy for Ptr<T> {}
impl<T> Clone for Ptr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::ops::Deref for Ptr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T> std::ops::DerefMut for Ptr<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.get()
    }
}

unsafe impl<T> Send for Ptr<T> {}
unsafe impl<T> Sync for Ptr<T> {}
