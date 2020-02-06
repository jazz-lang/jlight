pub mod ieiunium;
pub mod mem;
pub mod parallel;
pub mod serial;
use crate::runtime::threads::JThread;
use crate::util::shared::*;
use std::sync::atomic::Ordering;

use crate::runtime::{object::*, state::*, value::*};
pub trait GarbageCollector {
    fn allocate(&self, _: Object) -> Value;
    fn minor_collection(&self, _: &State);
    fn major_collection(&self, _: &State);
    fn collecting(&self) -> bool {
        false
    }

    fn should_collect(&self) -> bool;
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
            if state.gc.collecting() {
                trace!("Safepoint reached, waiting GC to finish");
                let mut blocking = STW.blocking.lock();
                assert!(*blocking > 0);
                *blocking -= 1;
                if *blocking == 0 {
                    STW.cnd.notify_all();
                }
                let mut attempt = 0;
                while state.gc.collecting() {
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
                state.gc.minor_collection(state);
            } else {
                trace!("Safepoint reached, no need for collection");
            }
        }
    } else {
        pub fn safepoint(state: &State) {
            if state.gc.should_collect() {
                state.gc.minor_collection(state);
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
