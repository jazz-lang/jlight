use super::tracer::*;
use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::threads::JThread;
use crate::runtime::value::*;

use crate::util::shared::*;
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

    pub fn allocate(&self, object: Object) -> crate::runtime::value::Value {
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

impl Drop for GlobalCollector {
    fn drop(&mut self) {
        //let mut heap = self.heap.lock();
        //let heap: &mut Vec<ObjectPointer> = &mut *heap;
        //while let Some(value) = heap.pop() {
        //let _ = value.finalize();
        //}
    }
}

lazy_static::lazy_static! {
    static ref STW: StopTheWorld = StopTheWorld {
        blocking: Mutex::new(0),
        cnd: Condvar::new()
    };
}

const GC_YIELD_MAX_ATTEMPT: u64 = 2;

cfg_if::cfg_if!(
    if #[cfg(feature="multithreaded")] {
        
        /// Stop-the-World pause mechanism. During safepoint all threads running jlight vm bytecode are suspended.
        pub fn safepoint(state: &State) {
            if state.gc.collecting.load(Ordering::Relaxed) {
                trace!("Safepoint reached, waiting GC to finish");
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
                trace!("Safepoint reached, triggering collection");
                state.gc.collect(state);
            } else {
                trace!("Safepoint reached, no need for collection");
            }
        }
    } else {
        pub fn safepoint(state: &State) {
            if state.gc.should_collect() {
                state.gc.collect(state);
            }
        }
    }
);

struct StopTheWorld {
    blocking: Mutex<usize>,
    cnd: Condvar,
}
cfg_if::cfg_if!(
    if #[cfg(feature="multithreaded")] {
        fn stop_the_world<F: FnMut(&JThread)>(state: &State, mut cb: F) {
            // lock threads from starting or exiting
            let threads = state.threads.threads.lock();
            std::sync::atomic::fence(Ordering::SeqCst);

            let mut blocking = STW.blocking.lock();

            let native_threads_count =
                threads
                    .iter()
                    .fold(0, |c, x| if x.local_data().native { c + 1 } else { c });
            *blocking = native_threads_count - 1;
            while *blocking > 0 {
                trace!("STW: Waiting for {} thread(s)", blocking);
                STW.cnd.wait(&mut blocking);
            }

            for thread in threads.iter() {
                cb(thread);
            }
        }
    } else {
        fn stop_the_world<F: FnMut(&JThread)>(state: &State,mut cb: F) {
            let threads = state.threads.threads.lock();
            for thread in threads.iter() {
                cb(thread);
            }
        }
    }
);
