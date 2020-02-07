pub mod freelist;

use crate::heap::mem::*;
use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::value::*;
use crate::util::shared::*;
use crate::util::tagged_pointer::*;
use freelist::{FreeList, FreeSpace};
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static IEIUNIUM_COLLECTING: AtomicBool = AtomicBool::new(false);

pub struct IeiuniumCollectorInner {
    pub heap: Vec<ObjectPointer>,
    pub memory_heap: Region,
    pub sweep_alloc: SweepAllocator,
    pub gray: Mutex<LinkedList<ObjectPointerPointer>>,
    pub threshold: usize,
    pub major_threshold: usize,
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
            threshold: 4096,
            major_threshold: 4096 * 2,
            bytes_allocated: 0,
            sweep_alloc: SweepAllocator::new(heap),
        }
    }

    fn fragmentation(&self) -> f32 {
        self.sweep_alloc.free_list.fragmentation()
    }

    fn minor(&mut self, state: &State) {
        trace!("Ieinium GC: Minor collection triggered");
        let mut gray = self.gray.lock();
        let mut rootset = vec![];
        super::stop_the_world(state, |thread| {
            thread.each_pointer(|x| {
                rootset.push(x);
            });
        });
        let mut incremental = IncrementalMarkAndSweep {
            state: IncrementalState::Roots,
            gray: &mut *gray,
            heap_region: self.memory_heap,
            freelist: FreeList::new(),
            rootset: &rootset,
            bytes_allocated: &mut self.bytes_allocated,
            used_end: self.sweep_alloc.top,
        };

        incremental.collect(512);
        self.sweep_alloc.free_list = incremental.freelist;
        trace!("Ieinium GC: Minor collection finished!");
    }
    fn major(&mut self, state: &State) {
        IEIUNIUM_COLLECTING.store(true, Ordering::Release);
        trace!("Ieiunium GC: Triggering major collection");
        let mut rootset = vec![];
        super::stop_the_world(state, |thread| {
            thread.each_pointer(|optr| {
                rootset.push(optr);
            });
        });
        let fragmentation = self.sweep_alloc.free_list.fragmentation();
        let mut mc = MarkCompact {
            rootset: &rootset,
            gray: LinkedList::new(),
            heap: self.memory_heap,
            freelist: FreeList::new(),
            init_top: self.sweep_alloc.top,
            top: self.memory_heap.end,
            bytes_allocated: &mut self.bytes_allocated,
            used_end: self.sweep_alloc.top,
        };

        mc.collect(fragmentation);
        if fragmentation < 0.40 {
            self.sweep_alloc.free_list = mc.freelist;
        } else {
            self.sweep_alloc.top = mc.top;
            self.sweep_alloc.free_list = FreeList::new();
            self.sweep_alloc.limit = self.memory_heap.end;
        }
        trace!("Ieinium GC: Major collection finished");
        IEIUNIUM_COLLECTING.store(false, Ordering::Release);
    }
    fn alloc(&mut self, state: &State, object: Object) -> Value {
        if self.threshold < self.bytes_allocated {
            if !self.gray.lock().is_empty() {
                self.minor(state);
                if self.bytes_allocated as f64 > self.threshold as f64 * 0.7 {
                    self.threshold = (self.bytes_allocated as f64 / 0.7) as usize;
                }
            }
        }
        let mut ptr = self.sweep_alloc.allocate(std::mem::size_of::<Object>());
        self.bytes_allocated += std::mem::size_of::<Object>();
        if ptr.is_null() || self.major_threshold < self.bytes_allocated {
            self.major(state);
            if self.bytes_allocated as f64 > self.major_threshold as f64 * 0.3 {
                self.major_threshold = (self.bytes_allocated as f64 / 0.3) as usize;
            }
            if ptr.is_null() {
                ptr = self.sweep_alloc.allocate(std::mem::size_of::<Object>());
            }
        }
        unsafe {
            ptr.to_mut_ptr::<Object>().write(object);
        }
        let optr = ObjectPointer {
            raw: TaggedPointer::new(ptr.to_mut_ptr::<Object>()),
        };
        optr.set_color(COLOR_GREY);
        self.gray.lock().push_back(optr.pointer());
        let value = Value::from(optr);
        value
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
    rootset: &'a [ObjectPointerPointer],
    heap_region: Region,
    freelist: FreeList,
    gray: &'a mut LinkedList<ObjectPointerPointer>,
    state: IncrementalState,
    bytes_allocated: &'a mut usize,
    used_end: Address,
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
                    if root.get().get_color() == COLOR_GREY {
                        continue;
                    }
                    root.get().set_color(COLOR_GREY);
                    self.gray.push_back(*root);
                }

                self.state = IncrementalState::Mark;
                return 0;
            }
            IncrementalState::Mark => {
                if !self.gray.is_empty() {
                    let mut count = 0;
                    while let Some(object) = self.gray.pop_front() {
                        if !(count < limit) {
                            return count;
                        }
                        if object.raw.is_null() {
                            continue;
                        }
                        if object.get().is_null() {
                            continue;
                        }
                        object.get().set_color(COLOR_BLACK);
                        object.get().get().each_pointer(|pointer| {
                            pointer.get().set_color(COLOR_GREY);
                            self.gray.push_back(pointer);
                        });
                        count += 1;
                    }
                    self.state = IncrementalState::Sweep;
                    return count;
                } else {
                    self.state = IncrementalState::Sweep;
                    return 0;
                }
            }
            IncrementalState::Sweep => {
                let mut count = 0;
                let mut garbage_start = Address::null();

                let start = self.heap_region.start;
                let end = self.heap_region.end;
                let mut scan = start;
                const OBJECT_SIZE: usize = std::mem::size_of::<Object>();

                while scan < self.used_end {
                    let object = ObjectPointer {
                        raw: TaggedPointer {
                            raw: scan.to_mut_ptr::<Object>(),
                        },
                    };
                    count += 1;
                    if object.get_color() != COLOR_WHITE {
                        self.add_freelist(garbage_start, scan);
                        garbage_start = Address::null();
                        object.set_color(COLOR_WHITE);
                    } else if garbage_start.is_non_null() {
                        // more garbage, do nothing
                        *self.bytes_allocated -= std::mem::size_of::<Object>();
                        trace!("Ieinium GC: Minor sweepee 0x{:p}", scan.to_mut_ptr::<u8>());
                    } else {
                        trace!("Ieinium GC: Minor sweep 0x{:p}", scan.to_mut_ptr::<u8>());
                        *self.bytes_allocated -= std::mem::size_of::<Object>();
                        // start garbage, last object was live
                        garbage_start = scan;
                    }
                    scan = scan.offset(OBJECT_SIZE);
                }
                self.add_freelist(garbage_start, self.heap_region.end);

                self.state = IncrementalState::Done;
                return count;
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

pub struct MarkCompact<'a> {
    heap: Region,
    init_top: Address,
    top: Address,
    rootset: &'a [ObjectPointerPointer],
    gray: LinkedList<ObjectPointerPointer>,
    freelist: FreeList,
    bytes_allocated: &'a mut usize,
    used_end: Address,
}

impl<'a> MarkCompact<'a> {
    fn collect(&mut self, fragmentation: f32) {
        trace!("Ieinium GC: Marking...");
        self.mark_live();
        if fragmentation < 0.40 {
            trace!("Ieinium GC: Heap is not fragmented, sweeping...");
            self.sweep();
        } else {
            trace!("Ieinium GC: Heap is fragmented, compacting...");
            self.compute_forward();
            self.update_reference();
            self.relocate();
        }
    }
    fn mark_live(&mut self) {
        for root in self.rootset.iter() {
            root.get().set_color(COLOR_GREY);
            self.gray.push_back(*root);
        }

        while let Some(object) = self.gray.pop_front() {
            object.get().set_color(COLOR_BLACK);
            object.get().get().each_pointer(|field| {
                field.get().set_color(COLOR_GREY);
                self.gray.push_back(field);
            });
        }
    }

    fn sweep(&mut self) {
        let mut garbage_start = Address::null();

        let start = self.heap.start;
        let end = self.heap.end;
        let mut scan = start;
        const OBJECT_SIZE: usize = std::mem::size_of::<Object>();

        while scan < self.used_end {
            let object = ObjectPointer {
                raw: TaggedPointer {
                    raw: scan.to_mut_ptr::<Object>(),
                },
            };
            if object.get_color() == COLOR_BLACK {
                self.add_freelist(garbage_start, scan);
                garbage_start = Address::null();
                object.set_color(COLOR_WHITE);
            } else if garbage_start.is_non_null() {
                *self.bytes_allocated -= std::mem::size_of::<Object>();
                trace!(
                    "Ieinium GC: Major sweeped object 0x{:p}",
                    scan.to_ptr::<u8>()
                );
            // more garbage, do nothing
            } else {
                *self.bytes_allocated -= std::mem::size_of::<Object>();
                // start garbage, last object was live
                garbage_start = scan;
                trace!(
                    "Ieinium GC: Major sweeped object 0x{:p}",
                    scan.to_ptr::<u8>()
                );
            }
            scan = scan.offset(OBJECT_SIZE);
        }
        self.add_freelist(garbage_start, self.heap.end);
    }
    fn compute_forward(&mut self) {
        self.walk_heap(|mc, object, _addr| {
            if object.get_color() == COLOR_BLACK {
                let fwd = mc.allocate(std::mem::size_of::<Object>());
                object.get_mut().fwdptr = fwd;
            }
        });
    }

    fn relocate(&mut self) {
        self.walk_heap(|_, object, address| {
            if object.get_color() == COLOR_BLACK {
                let dest = object.get().fwdptr;
                if address != dest {
                    trace!(
                        "Ieinium GC: Move object from 0x{:p} to 0x{:p}",
                        object.raw.raw,
                        dest.to_mut_ptr::<u8>()
                    );
                    object.get().copy_to(dest);
                }

                let dest_obj = dest.to_mut_ptr::<Object>();
                unsafe {
                    (*dest_obj).color.store(COLOR_WHITE, Ordering::Release);
                }
            } else {
                object.finalize();
            }
        });
    }
    fn update_reference(&mut self) {
        self.walk_heap(|mc, object, _addr| {
            if object.get_color() == COLOR_BLACK {
                object.get().each_pointer(|optr| {
                    mc.forward_reference(optr);
                });
            }
        });
        for root in self.rootset.iter() {
            self.forward_reference(*root);
        }
    }

    fn forward_reference(&mut self, slot: ObjectPointerPointer) {
        let fwd_addr = slot.get().get().fwdptr;
        *slot.get_mut() = ObjectPointer {
            raw: TaggedPointer::new(fwd_addr.to_mut_ptr::<Object>()),
        };
    }

    fn walk_heap<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Self, ObjectPointer, Address),
    {
        let start = self.heap.start;
        let end = self.init_top;

        let mut scan = start;
        while scan < end {
            let object = scan.to_mut_ptr::<Object>();

            let object_size = std::mem::size_of::<Object>();

            f(
                self,
                ObjectPointer {
                    raw: TaggedPointer::new(object),
                },
                scan,
            );

            scan = scan.offset(object_size);
        }
    }

    fn allocate(&mut self, object_size: usize) -> Address {
        let addr = self.top;
        let next = self.top.offset(object_size);

        if next <= self.heap.end {
            self.top = next;
            return addr;
        }

        panic!("FAIL: Not enough space for objects.");
    }

    fn add_freelist(&mut self, start: Address, end: Address) {
        if start.is_null() {
            return;
        }

        let size = end.offset_from(start);
        self.freelist.add(start, size);
    }
}

pub struct IeiuniumCollector {
    inner: RwLock<IeiuniumCollectorInner>,
}

impl IeiuniumCollector {
    pub fn new(size: usize) -> Self {
        Self {
            inner: RwLock::new(IeiuniumCollectorInner::new(size)),
        }
    }
}

impl super::GarbageCollector for IeiuniumCollector {
    fn allocate(&self, state: &State, object: Object) -> Value {
        self.inner.write().alloc(state, object)
    }

    fn major_collection(&self, state: &State) {
        self.inner.write().major(state);
    }

    fn minor_collection(&self, state: &State) {
        self.inner.write().minor(state);
    }

    fn write_barrier(&self, parent: ObjectPointer, child: ObjectPointer) -> bool {
        let should_emit_barrier =
            parent.get_color() == COLOR_BLACK && child.get_color() == COLOR_WHITE;
        if !should_emit_barrier {
            return false;
        }
        parent.set_color(COLOR_GREY);
        self.inner.read().gray.lock().push_back(parent.pointer());
        true
    }

    fn should_collect(&self) -> bool {
        let inner = self.inner.read();
        inner.bytes_allocated > inner.major_threshold
    }
}
