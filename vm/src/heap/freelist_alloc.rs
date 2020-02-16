use super::freelist::*;
use super::space::Space;
use crate::util::mem::{Address, Region};
pub struct FreeListAllocator {
    pub space: Space,
    pub freelist: FreeList,
}

impl FreeListAllocator {
    pub fn new(heap: Space) -> Self {
        Self {
            space: heap,
            freelist: FreeList::new(),
        }
    }

    pub fn allocate(&mut self, size: usize) -> Address {
        if self.space.may_allocate_in_current(size) {
            // if it possible to allocate in current page we should do it
            return self.space.allocate(size, &mut false);
        }
        // We cannot allocate in current page, let's try to find free slot.
        let (free_space, size) = self.freelist.alloc(size);

        if free_space.is_non_null() {
            let object = free_space.addr();
            let free_size = size;
            assert!(size <= free_size);

            let free_start = object.offset(size);
            let free_end = object.offset(free_size);
            let new_free_size = free_end.offset_from(free_start);
            if new_free_size != 0 {
                self.freelist.add(free_start, new_free_size);
            }
            return object;
        }
        // Free slot not found, allocate new page and allocate memory in new page.
        self.space.add_page(self.space.page_size);
        self.space.allocate(size, &mut false)
    }

    pub fn free(&mut self, pointer: Address, size: usize) {
        self.freelist.add(pointer.sub(size), size);
    }
}
