use super::block::{LINES_PER_BLOCK, OBJECTS_PER_BLOCK};
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};
/// A bytemap for marking objects when they are traced.
pub struct ObjectMap {
    values: [AtomicU8; OBJECTS_PER_BLOCK],
}
pub struct LineMap {
    values: [AtomicU8; LINES_PER_BLOCK],
    mark_value: u8,
}

pub trait Bytemap {
    fn values(&self) -> &[AtomicU8];
    fn values_mut(&mut self) -> &mut [AtomicU8];

    fn reset(&mut self);

    /// The value to use for marking an entry.
    fn mark_value(&self) -> u8 {
        1
    }

    /// Sets the given index in the bytemap.
    fn set(&mut self, index: usize) {
        self.values()[index].store(self.mark_value(), Ordering::Release);
    }

    /// Unsets the given index in the bytemap.
    fn unset(&mut self, index: usize) {
        self.values()[index].store(0, Ordering::Release);
    }

    /// Returns `true` if a given index is set.
    fn is_set(&self, index: usize) -> bool {
        self.values()[index].load(Ordering::Acquire) > 0
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_ptr_alignment))]
    fn is_empty(&self) -> bool {
        let mut offset = 0;

        while offset < self.values().len() {
            // The cast to *mut usize here is important so that reads read a
            // single word, not a byte.
            let value = unsafe {
                let ptr = self.values().as_ptr().add(offset) as *const usize;

                *ptr
            };

            if value > 0 {
                return false;
            }

            offset += mem::size_of::<usize>();
        }

        true
    }

    fn len(&mut self) -> usize {
        let mut amount = 0;

        for value in self.values_mut().iter_mut() {
            if *value.get_mut() > 0 {
                amount += 1;
            }
        }

        amount
    }
}

impl ObjectMap {
    /// Returns a new, empty object bytemap.
    pub fn new() -> ObjectMap {
        let values = [0_u8; OBJECTS_PER_BLOCK];

        ObjectMap {
            values: unsafe { mem::transmute(values) },
        }
    }
}

impl LineMap {
    /// Returns a new, empty line bytemap.
    pub fn new() -> LineMap {
        let values = [0_u8; LINES_PER_BLOCK];

        LineMap {
            values: unsafe { mem::transmute(values) },
            mark_value: 1,
        }
    }

    pub fn swap_mark_value(&mut self) {
        if self.mark_value == 1 {
            self.mark_value = 2;
        } else {
            self.mark_value = 1;
        }
    }

    /// Resets marks from previous marking cycles.
    pub fn reset_previous_marks(&mut self) {
        for index in 0..LINES_PER_BLOCK {
            let current = self.values[index].get_mut();

            if *current != self.mark_value {
                *current = 0;
            }
        }
    }
}

impl Bytemap for ObjectMap {
    #[inline(always)]
    fn values(&self) -> &[AtomicU8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [AtomicU8] {
        &mut self.values
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.values.as_mut_ptr(), 0, OBJECTS_PER_BLOCK);
        }
    }
}

impl Bytemap for LineMap {
    #[inline(always)]
    fn values(&self) -> &[AtomicU8] {
        &self.values
    }

    #[inline(always)]
    fn values_mut(&mut self) -> &mut [AtomicU8] {
        &mut self.values
    }

    fn mark_value(&self) -> u8 {
        self.mark_value
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.values.as_mut_ptr(), 0, LINES_PER_BLOCK);
        }
    }
}
