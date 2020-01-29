//! Chunks of memory allowing for Vec-like operations.
//!
//! A Chunk is a region of memory of a given type, with a fixed amount of
/// values. Chunks are optimized for performance, sacrificing safety in the
/// process.
///
/// Chunks do not drop the individual values. This means that code using a Chunk
/// must take care of this itself.
use std::alloc::{self, Layout};
use std::mem;
use std::ops::Drop;
use std::ops::{Index, IndexMut};
use std::ptr;

pub struct Chunk<T> {
    ptr: *mut T,
    capacity: usize,
}

unsafe fn layout_for<T>(capacity: usize) -> Layout {
    Layout::from_size_align_unchecked(mem::size_of::<T>() * capacity, mem::align_of::<T>())
}

#[cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]
impl<T> Chunk<T> {
    pub fn new(capacity: usize) -> Self {
        if capacity == 0 {
            return Chunk {
                ptr: ptr::null_mut(),
                capacity: 0,
            };
        }

        let layout = unsafe { layout_for::<T>(capacity) };
        let ptr = unsafe { alloc::alloc(layout) as *mut T };

        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }

        let mut chunk = Chunk { ptr, capacity };

        chunk.reset();
        chunk
    }

    pub fn len(&self) -> usize {
        self.capacity
    }

    pub fn reset(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                // We need to zero out the memory as otherwise we might get random
                // garbage.
                ptr::write_bytes(self.ptr, 0, self.capacity);
            }
        }
    }
}

impl<T> Drop for Chunk<T> {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                alloc::dealloc(self.ptr as *mut u8, layout_for::<T>(self.len()));
            }
        }
    }
}

impl<T> Index<usize> for Chunk<T> {
    type Output = T;

    fn index(&self, offset: usize) -> &T {
        unsafe { &*self.ptr.add(offset) }
    }
}

impl<T> IndexMut<usize> for Chunk<T> {
    fn index_mut(&mut self, offset: usize) -> &mut T {
        unsafe { &mut *self.ptr.add(offset) }
    }
}
