use std::cmp;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The inner value of a pointer.
///
/// This uses the C representation to ensure that the value is always the first
/// member of this structure. This in turn allows one to read the value of this
/// `Inner` using `*mut T`.
#[repr(C)]
pub struct Inner<T> {
    value: T,
    references: AtomicUsize,
}

/// A thread-safe reference counted pointer.
pub struct Arc<T> {
    inner: NonNull<Inner<T>>,
}

unsafe impl<T> Sync for Arc<T> {}
unsafe impl<T> Send for Arc<T> {}

impl<T> Arc<T> {
    /// Consumes the `Arc`, returning the wrapped pointer.
    ///
    /// The returned pointer is in reality a pointer to the inner structure,
    /// instead of a pointer directly to the value.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::wrong_self_convention))]
    pub fn into_raw(value: Self) -> *mut T {
        let raw = value.inner;

        mem::forget(value);

        raw.as_ptr() as _
    }

    /// Constructs an `Arc` from a raw pointer.
    ///
    /// This method is incredibly unsafe, as it makes no attempt to verify if
    /// the pointer actually a pointer previously created using
    /// `Arc::into_raw()`.
    pub unsafe fn from_raw(value: *mut T) -> Self {
        Arc {
            inner: NonNull::new_unchecked(value as *mut Inner<T>),
        }
    }

    pub fn new(value: T) -> Self {
        let inner = Inner {
            value,
            references: AtomicUsize::new(1),
        };

        Arc {
            inner: unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(inner))) },
        }
    }

    pub fn inner(&self) -> &Inner<T> {
        unsafe { self.inner.as_ref() }
    }

    pub fn references(&self) -> usize {
        self.inner().references.load(Ordering::SeqCst)
    }

    pub fn as_ptr(&self) -> *mut T {
        self.inner.as_ptr() as _
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner().value
    }
}

impl<T> DerefMut for Arc<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut self.inner.as_mut().value }
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        self.inner().references.fetch_add(1, Ordering::Relaxed);

        Arc { inner: self.inner }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        unsafe {
            if self.inner().references.fetch_sub(1, Ordering::AcqRel) == 1 {
                let boxed = Box::from_raw(self.inner.as_mut());

                drop(boxed);
            }
        }
    }
}

impl<T: PartialOrd> PartialOrd for Arc<T> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for Arc<T> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: PartialEq> PartialEq for Arc<T> {
    fn eq(&self, other: &Self) -> bool {
        (**self) == (**other)
    }
}

impl<T: Eq> Eq for Arc<T> {}
