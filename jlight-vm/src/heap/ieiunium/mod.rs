pub mod freelist;

use crate::heap::mem::*;
use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::value::*;
use crate::util::shared::*;
use freelist::{FreeList, FreeSpace};
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub struct IeiuniumCollectorInner {
    pub heap: Vec<ObjectPointer>,
    pub memory_heap: Region,
    pub sweep_alloc: SweepAllocator,
    pub gray: Mutex<LinkedList<ObjectPointer>>,
    pub white: Mutex<LinkedList<ObjectPointer>>,
    pub black: Mutex<LinkedList<ObjectPointer>>,
    pub threshold: usize,
    pub bytes_allocated: usize,
}

impl IeiuniumCollectorInner {
    pub fn new(size: usize) -> Self {
        let heap_start = commit(size, false);
        let heap_end = heap_start.offset(size);
        let heap = Region::new(heap_start, heap_end);
        Self {
            heap: vec![],
            memory_heap: heap,
            gray: Mutex::new(Default::default()),
            white: Mutex::new(Default::default()),
            black: Mutex::new(Default::default()),
            threshold: 4096,
            bytes_allocated: 0,
            sweep_alloc: SweepAllocator::new(heap),
        }
    }

    fn fragmentation(&self) -> f32 {
        self.sweep_alloc.free_list.fragmentation()
    }

    fn alloc(&mut self, object: Object) -> Value {
        if self.threshold < self.bytes_allocated {
            if self.bytes_allocated as f64 > self.threshold as f64 * 0.7 {
                self.threshold = (self.bytes_allocated as f64 / 0.7) as usize;
            }
        }
        unimplemented!()
    }
}

#[derive(Clone, PartialEq, Eq, Copy, Debug)]
#[repr(u8)]
pub enum IncrementalState {
    Done,
    Mark,
    Sweep,
    Roots,
}

pub struct IncrementalMarkAndSweep<'a> {
    rootset: &'a [ObjectPointer],
    heap_region: Region,
    freelist: FreeList,
    gray: &'a mut LinkedList<ObjectPointer>,
    black: &'a mut LinkedList<ObjectPointer>,
    heap: &'a [ObjectPointer],
    new_heap: ObjectPointer,
    state: IncrementalState,
}

impl<'a> IncrementalMarkAndSweep<'a> {
    pub fn collect(&mut self, limit: usize) {
        let mut result = 0;
        while result < limit {
            result += self.step(limit);
            if self.state == IncrementalState::Done {
                break;
            }
        }
    }

    fn add_freelist(&mut self, start: Address, end: Address) {
        if start.is_null() {
            return;
        }

        let size = end.offset_from(start);
        self.freelist.add(start, size);
    }

    fn step(&mut self, limit: usize) -> usize {
        match &self.state {
            IncrementalState::Roots => {
                for root in self.rootset.iter() {
                    if root.get_color() == COLOR_GREY {
                        continue;
                    }
                    self.gray.push_back(*root);
                }
                root.set_color(COLOR_GREY);
                self.state = IncrementalState::Mark;
                return 0;
            }
            IncrementalState::Mark => {
                if !self.gray.is_empty() {
                } else {
                    self.state = IncrementalState::Sweep;
                }
                unimplemented!()
            }
            _ => unimplemented!(),
        }
    }
}

pub struct SweepAllocator {
    top: Address,
    limit: Address,
    free_list: FreeList,
}

impl SweepAllocator {
    fn new(heap: Region) -> SweepAllocator {
        SweepAllocator {
            top: heap.start,
            limit: heap.end,
            free_list: FreeList::new(),
        }
    }

    fn allocate(&mut self, size: usize) -> Address {
        let object = self.top;
        let next_top = object.offset(size);

        if next_top <= self.limit {
            self.top = next_top;
            return object;
        }

        let (free_space, size) = self.free_list.alloc(size);

        if free_space.is_non_null() {
            let object = free_space.addr();
            let free_size = size;
            assert!(size <= free_size);

            let free_start = object.offset(size);
            let free_end = object.offset(free_size);
            let new_free_size = free_end.offset_from(free_start);
            if new_free_size != 0 {
                self.free_list.add(free_start, new_free_size);
            }
            return object;
        }

        Address::null()
    }
}
