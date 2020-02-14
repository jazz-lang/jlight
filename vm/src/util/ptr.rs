use std::ops::{Deref, DerefMut};
pub struct Ptr<T: ?Sized> {
    pub raw: *mut T,
}

impl<T: Sized> Ptr<T> {
    pub fn new(x: T) -> Self {
        Self {
            raw: std::boxed::Box::into_raw(std::boxed::Box::new(x)),
        }
    }
    pub fn take(&self) -> T
    where
        T: Default,
    {
        assert!(!self.raw.is_null());
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
