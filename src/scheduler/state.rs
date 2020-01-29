use crate::sync::{
    park::ParkGroup,
    queue::{Queue, RcQueue},
};

use crossbeam_deque::{Injector, Steal};
use std::iter;
use std::sync::atomic::{AtomicBool, Ordering};

/// The maximum number of threads a single pool allows.
const MAX_THREADS: usize = 255;

/// The internal state of a single pool, shared between the many workers that
/// belong to the pool.
pub struct PoolState<T: Send> {
    /// The queues available for workers to store work in and steal work from.
    pub queues: Vec<RcQueue<T>>,

    /// A boolean indicating if the scheduler is alive, or should shut down.
    alive: AtomicBool,

    /// The global queue on which new jobs will be scheduled,
    global_queue: Injector<T>,

    /// Used for parking and unparking worker threads.
    park_group: ParkGroup,
}

impl<T: Send> PoolState<T> {
    /// Creates a new state for the given number worker threads.
    ///
    /// Threads are not started by this method, and instead must be started
    /// manually.
    pub fn new(mut threads: usize) -> Self {
        if threads > MAX_THREADS {
            threads = MAX_THREADS;
        }

        let queues = iter::repeat_with(Queue::with_rc).take(threads).collect();

        PoolState {
            alive: AtomicBool::new(true),
            queues,
            global_queue: Injector::new(),
            park_group: ParkGroup::new(),
        }
    }

    /// Schedules a new job onto the global queue.
    pub fn push_global(&self, value: T) {
        self.global_queue.push(value);
        self.park_group.notify_one();
    }

    /// Schedules a job onto a specific queue.
    ///
    /// This method will panic if the queue index is invalid.
    pub fn schedule_onto_queue(&self, queue: usize, value: T) {
        self.queues[queue].push_external(value);

        // A worker might be parked when sending it an external message, so we
        // have to wake them up. We have to notify all workers instead of a
        // single one, otherwise we may end up notifying a different worker.
        self.park_group.notify_all();
    }

    /// Pops a value off the global queue.
    ///
    /// This method will block the calling thread until a value is available.
    pub fn pop_global(&self) -> Option<T> {
        loop {
            match self.global_queue.steal() {
                Steal::Empty => {
                    return None;
                }
                Steal::Retry => {}
                Steal::Success(value) => {
                    return Some(value);
                }
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn terminate(&self) {
        self.alive.store(false, Ordering::Release);
        self.notify_all();
    }

    pub fn notify_all(&self) {
        self.park_group.notify_all();
    }

    /// Parks the current thread as long as the given condition is true.
    pub fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        self.park_group
            .park_while(|| self.is_alive() && condition());
    }

    /// Returns true if one or more jobs are present in the global queue.
    pub fn has_global_jobs(&self) -> bool {
        !self.global_queue.is_empty()
    }
}
