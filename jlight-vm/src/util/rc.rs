use super::ptr::Ptr;
use std::cmp;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
/// The inner value of a pointer.
///
/// This uses the C representation to ensure that the value is always the first
/// member of this structure. This in turn allows one to read the value of this
/// `Inner` using `*mut T`.
#[repr(C)]
pub struct Inner<T> {
    value: T,
    references: usize,
}

/// A reference counted pointer.
pub struct Rc<T> {
    inner: Ptr<Inner<T>>,
}

unsafe impl<T> Sync for Rc<T> {}
unsafe impl<T> Send for Rc<T> {}

impl<T> Rc<T> {
    /// Consumes the `Rc`, returning the wrapped pointer.
    ///
    /// The returned pointer is in reality a pointer to the inner structure,
    /// instead of a pointer directly to the value.
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::wrong_self_convention))]
    pub fn into_raw(value: Self) -> *mut T {
        let raw = value.inner;

        mem::forget(value);

        raw.0 as _
    }

    /// Constructs an `Rc` from a raw pointer.
    ///
    /// This method is incredibly unsafe, as it makes no attempt to verify if
    /// the pointer actually a pointer previously created using
    /// `Rc::into_raw()`.
    pub unsafe fn from_raw(value: *mut T) -> Self {
        Rc {
            inner: Ptr(value as *mut Inner<T>),
        }
    }

    pub fn new(value: T) -> Self {
        let inner = Inner {
            value,
            references: 1,
        };

        Rc {
            inner: Ptr::new(inner),
        }
    }

    pub fn inner(&self) -> &Inner<T> {
        self.inner.get()
    }

    pub fn inner_mut(&self) -> &mut Inner<T> {
        self.inner.get()
    }

    pub fn references(&self) -> usize {
        self.inner().references
    }

    pub fn as_ptr(&self) -> *mut T {
        self.inner.0 as _
    }

    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.as_ptr() == other.as_ptr()
    }
}

impl<T> Deref for Rc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner().value
    }
}

impl<T> DerefMut for Rc<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner.get().value
    }
}

impl<T> Clone for Rc<T> {
    fn clone(&self) -> Rc<T> {
        self.inner_mut().references += 1;

        Rc { inner: self.inner }
    }
}

impl<T> Drop for Rc<T> {
    fn drop(&mut self) {
        unsafe {
            self.inner_mut().references -= 1;
            if self.inner_mut().references == 1 {
                let _ = Box::from_raw(self.inner.0);
            }
        }
    }
}

impl<T: PartialOrd> PartialOrd for Rc<T> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for Rc<T> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: PartialEq> PartialEq for Rc<T> {
    fn eq(&self, other: &Self) -> bool {
        (**self) == (**other)
    }
}

impl<T: Eq> Eq for Rc<T> {}

use std::hash::{Hash, Hasher};

impl<T: Hash> Hash for Rc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        assert!(!self.inner.is_null());
        self.inner.get().value.hash(state);
    }
}

use std::fmt;

impl<T: fmt::Debug> fmt::Debug for Rc<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", **self)
    }
}

impl<T: fmt::Display> fmt::Display for Rc<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", **self)
    }
}
