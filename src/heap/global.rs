use super::tracer::*;
use crate::runtime::object::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
const INITIAL_THRESHOLD: usize = 100;

// after collection we want the the ratio of used/total to be no
// greater than this (the threshold grows exponentially, to avoid
// quadratic behavior when the heap is growing linearly with the
// number of `new` calls):
const USED_SPACE_RATIO: f64 = 0.7;

pub struct GlobalCollector {
    heap: Mutex<Vec<ObjectPointer>>,
    threshold: AtomicUsize,
    bytes_allocated: AtomicUsize,
    pool: Arc<Pool>,
    collecting: AtomicBool,
    blocking: Mutex<usize>,
    reached_zero: parking_lot::Condvar,
}

impl GlobalCollector {
    pub fn sweep(&self) {
        let mut heap = self.heap.lock();
        let mut allocated = self.bytes_allocated.load(Ordering::Acquire);
        heap.retain(|object| {
            let retain = object.is_marked();
            if retain {
                object.unmark();
            } else {
                allocated -= std::mem::size_of::<Object>();
                object.finalize();
            }

            retain
        });
        if allocated as f64 > self.threshold.load(Ordering::Acquire) as f64 * USED_SPACE_RATIO {
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

    pub fn collect(&self) {
        self.collecting.store(true, Ordering::Release);
        self.pool.run();
        self.sweep();
        self.collecting.store(false, Ordering::Release);
    }

    pub fn should_collect(&self) -> bool {
        self.bytes_allocated.load(Ordering::Acquire) > self.threshold.load(Ordering::Acquire)
    }

    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
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
            x
        }
    }
}

lazy_static::lazy_static! {
    pub static ref GLOBAL: GlobalCollector = GlobalCollector {
        heap: Mutex::new(vec![]),
        threshold: AtomicUsize::new(INITIAL_THRESHOLD),
        bytes_allocated: AtomicUsize::new(0),
        pool: Pool::new(num_cpus::get() / 2),
        collecting: AtomicBool::new(true),
        blocking: Mutex::new(0),
        reached_zero: parking_lot::Condvar::new()
    };
}

pub fn safepoint() {
    if GLOBAL.collecting.load(Ordering::Relaxed) {
        while GLOBAL.collecting.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
    }
    if GLOBAL.should_collect() {
        /*let mut blocking = GLOBAL.blocking.lock();
        while *blocking > 0 {
            GLOBAL.reached_zero.wait(&mut blocking);
        }*/
        GLOBAL.collect();
    }
}
