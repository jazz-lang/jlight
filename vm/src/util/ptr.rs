pub struct Box<T: ?Sized> {
    pub raw: *mut T,
}

impl<T: Sized> Box<T> {
    pub fn new(x: T) -> Self {
        Self {
            raw: unsafe {
                let ptr = super::mem::alloc::<T>();
                ptr.write(x);
                ptr
            },
        }
    }
}

impl<T: ?Sized> Box<T> {
    pub fn into_raw(x: Self) -> *mut T {
        x.raw
    }
    pub fn get(&self) -> &mut T {
        unsafe { &mut *self.raw }
    }
    pub fn from_raw(x: *mut T) -> Self {
        Self { raw: x }
    }
}

use std::ops::{Deref, DerefMut};

impl<T: ?Sized> Deref for Box<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.raw }
    }
}

impl<T: ?Sized> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.raw }
    }
}

use std::fmt;
impl<T: fmt::Display> fmt::Display for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", **self)
    }
}

impl<T: fmt::Debug> fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", **self)
    }
}

unsafe impl<T: Sized> intrusive_collections::IntrusivePointer<T> for Box<T> {
    unsafe fn from_raw(x: *const T) -> Self {
        Self::from_raw(x as *mut T)
    }
}

impl<T: ?Sized> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            std::ptr::drop_in_place(self.raw);
            super::mem::free(self.raw)
        }
    }
}

impl<T: Clone> Clone for Box<T> {
    fn clone(&self) -> Self {
        Self::new((**self).clone())
    }
}

pub struct Ptr<T: ?Sized> {
    pub raw: *mut T,
}

impl<T: Sized> Ptr<T> {
    pub fn new(x: T) -> Self {
        Self {
            raw: Box::into_raw(Box::new(x)),
        }
    }
    pub fn take(&self) -> T
    where
        T: Default,
    {
        unsafe { std::ptr::replace(self.raw, Default::default()) }
    }

    pub fn replace(&self, with: T) -> T {
        unsafe { std::ptr::replace(self.raw, with) }
    }

    pub fn read(&self) -> T {
        unsafe { std::ptr::read(self.raw) }
    }

    pub fn write(&self, value: T) {
        unsafe { std::ptr::write(self.raw, value) }
    }
}

impl<T: ?Sized> Ptr<T> {
    pub fn get(&self) -> &mut T {
        unsafe { &mut *self.raw }
    }
}

impl<T> Deref for Ptr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T> DerefMut for Ptr<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.get()
    }
}

impl<T> Copy for Ptr<T> {}
impl<T> Clone for Ptr<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T> Send for Ptr<T> {}
unsafe impl<T> Sync for Ptr<T> {}
