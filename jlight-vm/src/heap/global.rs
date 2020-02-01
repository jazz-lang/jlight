use super::tracer::*;
use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::threads::JThread;
use crate::util::arc::Arc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const INITIAL_THRESHOLD: usize = 100;

// after collection we want the the ratio of used/total to be no
// greater than this (the threshold grows exponentially, to avoid
// quadratic behavior when the heap is growing linearly with the
// number of `new` calls):
const USED_SPACE_RATIO: f64 = 0.7;

pub struct GlobalCollector {
    pub heap: Mutex<Vec<ObjectPointer>>,
    pub threshold: AtomicUsize,
    pub bytes_allocated: AtomicUsize,
    pub pool: Arc<Pool>,
    pub collecting: AtomicBool,
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
        stop_the_world(state, |thread| {
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

    pub fn should_collect(&self) -> bool {
        self.bytes_allocated.load(Ordering::Acquire) > self.threshold.load(Ordering::Acquire)
    }

    pub fn allocate(&self, object: Object) -> ObjectPointer {
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
    };

    static ref STW: StopTheWorld = StopTheWorld {
        blocking: Mutex::new(0),
        cnd: parking_lot::Condvar::new()
    };
}

const GC_YIELD_MAX_ATTEMPT: u64 = 2;
/// Stop-the-World pause mechanism. During safepoint all threads running jlight vm bytecode are suspended.
pub fn safepoint(state: &State) {
    if state.gc.collecting.load(Ordering::Relaxed) {
        let mut blocking = STW.blocking.lock();
        assert!(*blocking > 0);
        *blocking -= 1;
        if *blocking == 0 {
            STW.cnd.notify_all();
        }
        let mut attempt = 0;
        while state.gc.collecting.load(Ordering::Relaxed) {
            if attempt >= 2 {
                std::thread::sleep(std::time::Duration::from_micros(
                    (attempt - GC_YIELD_MAX_ATTEMPT) * 1000,
                ));
            } else {
                std::thread::yield_now();
            }
            attempt += 1;
        }
    } else if state.gc.should_collect() {
        state.gc.collect(state);
    }
}

struct StopTheWorld {
    blocking: Mutex<usize>,
    cnd: parking_lot::Condvar,
}

fn stop_the_world<F: FnMut(&JThread)>(state: &State, mut cb: F) {
    // lock threads from starting or exiting
    let threads = state.threads.threads.lock();
    std::sync::atomic::fence(Ordering::SeqCst);

    let mut blocking = STW.blocking.lock();
    *blocking = threads.len() - 1;
    while *blocking > 0 {
        STW.cnd.wait(&mut blocking);
    }

    for thread in threads.iter() {
        cb(thread);
    }
}
