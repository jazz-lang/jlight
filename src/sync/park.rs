use parking_lot::{Condvar, Mutex};
use std::time::Duration;

macro_rules! lock_and_notify {
    ($parker: expr, $method: ident) => {
        // We need to acquire the lock, otherwise we may try to notify threads
        // between them checking their condition and unlocking the lock.
        //
        // Acquiring the lock here prevents this from happening, as we can not
        // acquire it until all threads that are about to sleep unlock the lock
        // from on their end.
        let _lock = $parker.mutex.lock();

        $parker.cvar.$method();
    };
}

/// A type for parking and waking up multiple threads easily.
///
/// A ParkGroup can be used by multiple threads to park themselves, and by other
/// threads to wake up any parked threads.
///
/// Since a ParkGroup is not associated with a single value, threads must
/// pass some sort of condition to `ParkGroup::park_while()`.
pub struct ParkGroup {
    mutex: Mutex<()>,
    cvar: Condvar,
}

impl ParkGroup {
    pub fn new() -> Self {
        ParkGroup {
            mutex: Mutex::new(()),
            cvar: Condvar::new(),
        }
    }

    /// Notifies all parked threads.
    pub fn notify_all(&self) {
        lock_and_notify!(self, notify_all);
    }

    /// Notifies a single parked thread.
    pub fn notify_one(&self) {
        lock_and_notify!(self, notify_one);
    }

    /// Parks the current thread as long as the given condition is true.
    pub fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        let mut lock = self.mutex.lock();

        while condition() {
            self.cvar.wait(&mut lock);
        }
    }

    /// Parks the current thread as long as the given condition is true, until
    /// the timeout expires.
    ///
    /// The return value will be true if the wait timed out.
    pub fn park_while_with_timeout<F>(&self, timeout: Duration, condition: F) -> bool
    where
        F: Fn() -> bool,
    {
        let mut lock = self.mutex.lock();

        while condition() {
            if self.cvar.wait_for(&mut lock, timeout).timed_out() {
                return true;
            }
        }

        false
    }
}
