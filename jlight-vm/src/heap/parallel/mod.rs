pub mod tracer;

use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::threads::JThread;
use crate::runtime::value::*;
use tracer::*;

use crate::util::shared::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const INITIAL_THRESHOLD: usize = 4096; // 4kb;

// after collection we want the the ratio of used/total to be no
// greater than this (the threshold grows exponentially, to avoid
// quadratic behavior when the heap is growing linearly with the
// number of `new` calls):
const USED_SPACE_RATIO: f64 = 0.7;

/// Parallel mark&sweep GC.
///
/// This GC is stop-the-world mark&sweep. When collection happens
/// this GC uses additional worker threads to trace heap, it's recommended to use this GC if you have large heap.
pub struct ParallelCollector {
    pub heap: Mutex<Vec<ObjectPointer>>,
    pub threshold: AtomicUsize,
    pub bytes_allocated: AtomicUsize,
    pub pool: Arc<Pool>,
    pub collecting: AtomicBool,
}

impl ParallelCollector {
    pub fn new(workers: usize) -> Self {
        Self {
            heap: Mutex::new(vec![]),
            threshold: AtomicUsize::new(INITIAL_THRESHOLD),
            bytes_allocated: AtomicUsize::new(INITIAL_THRESHOLD),
            pool: Pool::new(workers),
            collecting: AtomicBool::new(false),
        }
    }
    pub fn sweep(&self) {
        let mut heap = self.heap.lock();
        let mut allocated = self.bytes_allocated.load(Ordering::Acquire);
        heap.retain(|object| {
            let retain = object.get_color() == COLOR_BLACK;
            if retain {
                object.set_color(COLOR_WHITE);
            } else {
                allocated -= std::mem::size_of::<Object>();
                trace!("GC: Sweep 0x{:p}", object.raw.raw);
                object.finalize();
            }

            retain
        });
        if allocated as f64 > self.threshold.load(Ordering::Relaxed) as f64 * USED_SPACE_RATIO {
            // we didn't collect enough, so increase the
            // threshold for next time, to avoid thrashing the
            // collector too much/behaving quadratically.
            self.threshold.store(
                (allocated as f64 / USED_SPACE_RATIO) as usize,
                Ordering::Relaxed,
            );
            self.bytes_allocated.store(allocated, Ordering::Relaxed);
        }
        drop(heap);
    }

    pub fn collect(&self, state: &State) {
        self.collecting.store(true, Ordering::Release);
        //let mut stack = vec![];
        state.each_pointer(|object| {
            self.pool.schedule(object);
        });
        super::stop_the_world(state, |thread: &crate::runtime::threads::JThread| {
            thread.each_pointer(|object| {
                //stack.push(object);
                self.pool.schedule(object);
            });
        });

        /*while let Some(object) = stack.pop() {
            if object.get().is_marked() {
                continue;
            }
            object.get_mut().mark();
            object.get().get().each_pointer(|object| {
                stack.push(object);
            });
        }*/
        self.pool.run();
        self.sweep();
        self.collecting.store(false, Ordering::Release);
    }

    pub fn should_collect_(&self) -> bool {
        self.bytes_allocated.load(Ordering::Acquire) > self.threshold.load(Ordering::Acquire)
    }

    pub fn allocate_(&self, object: Object) -> crate::runtime::value::Value {
        unsafe {
            let mut heap = self.heap.lock();
            let pointer = std::alloc::alloc(std::alloc::Layout::new::<Object>()).cast::<Object>();
            pointer.write(object);
            let x = ObjectPointer {
                raw: crate::util::tagged_pointer::TaggedPointer::new(pointer),
            };
            heap.push(x);
            self.bytes_allocated
                .fetch_add(std::mem::size_of::<Object>(), Ordering::Relaxed);
            drop(heap);
            Value::from(x)
        }
    }
}

impl Drop for ParallelCollector {
    fn drop(&mut self) {
        //let mut heap = self.heap.lock();
        //let heap: &mut Vec<ObjectPointer> = &mut *heap;
        //while let Some(value) = heap.pop() {
        //let _ = value.finalize();
        //}
    }
}

impl super::GarbageCollector for ParallelCollector {
    fn collecting(&self) -> bool {
        self.collecting.load(Ordering::Acquire)
    }
    fn minor_collection(&self, state: &State) {
        self.collect(state);
    }
    fn major_collection(&self, state: &State) {
        self.collect(state);
    }

    fn allocate(&self, object: Object) -> Value {
        self.allocate_(object)
    }

    fn should_collect(&self) -> bool {
        self.should_collect_()
    }
}
