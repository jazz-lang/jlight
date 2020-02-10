use super::scheduler::timeout::*;
use super::value::*;
use crate::interpreter::context::*;
use crate::util;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use util::arc::*;
use util::ptr::*;
use util::tagged;
use util::tagged::*;

/// The bit that is set to mark a process as being suspended.
const SUSPENDED_BIT: usize = 0;

/// An enum describing what rights a thread was given when trying to reschedule
/// a process.
pub enum RescheduleRights {
    /// The rescheduling rights were not obtained.
    Failed,

    /// The rescheduling rights were obtained.
    Acquired,

    /// The rescheduling rights were obtained, and the process was using a
    /// timeout.
    AcquiredWithTimeout(Arc<Timeout>),
}

impl RescheduleRights {
    pub fn are_acquired(&self) -> bool {
        match self {
            RescheduleRights::Failed => false,
            _ => true,
        }
    }

    pub fn process_had_timeout(&self) -> bool {
        match self {
            RescheduleRights::AcquiredWithTimeout(_) => true,
            _ => false,
        }
    }
}

pub struct LocalData {
    pub context: Ptr<Context>,
}

pub struct Process {
    pub local_data: Ptr<LocalData>,
    /// If the process is waiting for a message.
    waiting_for_message: AtomicBool,

    /// A marker indicating if a process is suspened, optionally including the
    /// pointer to the timeout.
    ///
    /// When this value is NULL, the process is not suspended.
    ///
    /// When the lowest bit is set to 1, the pointer may point to (after
    /// unsetting the bit) to one of the following:
    ///
    /// 1. NULL, meaning the process is suspended indefinitely.
    /// 2. A Timeout, meaning the process is suspended until the timeout
    ///    expires.
    ///
    /// While the type here uses a `TaggedPointer`, in reality the type is an
    /// `Arc<Timeout>`. This trick is needed to allow for atomic
    /// operations and tagging, something which isn't possible using an
    /// `Option<T>`.
    suspended: TaggedPointer<Timeout>,
}

impl Process {
    pub fn local_data(&self) -> &LocalData {
        self.local_data.get()
    }

    pub fn local_data_mut(&self) -> &mut LocalData {
        self.local_data.get()
    }

    pub fn pop_context(&self) -> bool {
        let local_data = self.local_data_mut();
        if let Some(parent) = local_data.context.parent.take() {
            let old = local_data.context;
            unsafe {
                std::ptr::drop_in_place(old.raw);
            }
            local_data.context = parent;

            false
        } else {
            true
        }
    }

    pub fn push_context(&self, context: Context) {
        let mut boxed = Ptr::new(context);
        let local_data = self.local_data_mut();
        let target = &mut local_data.context;

        std::mem::swap(target, &mut boxed);

        target.parent = Some(boxed);
    }

    pub fn context_mut(&self) -> &mut Context {
        self.local_data_mut().context.get()
    }

    pub fn context_ptr(&self) -> Ptr<Context> {
        self.local_data_mut().context
    }

    pub fn context(&self) -> &Context {
        self.local_data().context.get()
    }

    pub fn trace<F>(&self, cb: F)
    where
        F: FnMut(*const super::cell::CellPointer),
    {
        self.local_data().context.trace(cb);
    }

    pub fn suspend_with_timeout(&self, timeout: Arc<Timeout>) {
        let pointer = Arc::into_raw(timeout);
        let tagged = tagged::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn suspend_without_timeout(&self) {
        let pointer = ptr::null_mut();
        let tagged = tagged::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn is_suspended_with_timeout(&self, timeout: &Arc<Timeout>) -> bool {
        let pointer = self.suspended.atomic_load();

        tagged::untagged(pointer) == timeout.as_ptr()
    }

    /// Attempts to acquire the rights to reschedule this process.
    pub fn acquire_rescheduling_rights(&self) -> RescheduleRights {
        let current = self.suspended.atomic_load();

        if current.is_null() {
            RescheduleRights::Failed
        } else if self.suspended.compare_and_swap(current, ptr::null_mut()) {
            let untagged = tagged::untagged(current);

            if untagged.is_null() {
                RescheduleRights::Acquired
            } else {
                let timeout = unsafe { Arc::from_raw(untagged) };

                RescheduleRights::AcquiredWithTimeout(timeout)
            }
        } else {
            RescheduleRights::Failed
        }
    }
}

impl PartialEq for Arc<Process> {
    fn eq(&self, other: &Arc<Process>) -> bool {
        self.as_ptr() == other.as_ptr()
    }
}
