use crate::context::*;
use crate::object::*;
use crate::runtime::machine::*;
use crate::scheduler::timeout::*;
use crate::sync::*;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
use std::i64;
use std::mem;
use std::ops::Drop;
use std::panic::RefUnwindSafe;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

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
    pub allocator: crate::heap::local_allocator::LocalAllocator,
    pub context: Box<Context>,
    pub thread_id: Option<u8>,
    pub mailbox: Mutex<Mailbox>,
    pub status: ProcessStatus,
    pub catch_entries: Vec<CatchEntry>,
}

pub struct Process {
    local_data: UnsafeCell<LocalData>,

    /// If the process is waiting for a message.
    waiting_for_message: AtomicBool,
    suspended: crate::tagged_pointer::TaggedPointer<Timeout>,
}

pub type RcProcess = Arc<Process>;

unsafe impl Sync for LocalData {}
unsafe impl Send for LocalData {}
unsafe impl Sync for Process {}
impl RefUnwindSafe for Process {}

impl Process {
    /// Write barrier for tracking cross generation writes.
    ///
    /// This barrier is based on the Steele write barrier and tracks the object
    /// that is *written to*, not the object that is being written.
    pub fn write_barrier(&self, written_to: ObjectPointer, written: ObjectPointer) {
        if written_to.is_mature() && written.is_young() {
            self.local_data_mut().allocator.remember_object(written_to);
        }
    }

    pub fn catch_entries(&self) -> &[CatchEntry] {
        &self.local_data().catch_entries
    }

    pub fn catch_entries_mut(&self) -> &mut Vec<CatchEntry> {
        &mut self.local_data_mut().catch_entries
    }

    pub fn context_mut(&self) -> &mut Context {
        &mut self.local_data_mut().context
    }

    pub fn should_collect_young_generation(&self) -> bool {
        self.local_data().allocator.should_collect_young()
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data().allocator.should_collect_mature()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn push_context(&self, context: Context) {
        let mut boxed = Box::new(context);
        let local_data = self.local_data_mut();
        let target = &mut local_data.context;

        mem::swap(target, &mut boxed);

        target.parent = Some(boxed);
    }

    /// Pops an execution context.
    ///
    /// This method returns true if we're at the top of the execution context
    /// stack.
    pub fn pop_context(&self) -> bool {
        let local_data = self.local_data_mut();

        if let Some(parent) = local_data.context.parent.take() {
            local_data.context = parent;

            false
        } else {
            true
        }
    }

    pub fn prepare_for_collection(&self, mature: bool) -> bool {
        self.local_data_mut()
            .allocator
            .prepare_for_collection(mature)
    }

    pub fn reclaim_blocks(&self, state: &crate::state::State, mature: bool) {
        self.local_data_mut()
            .allocator
            .reclaim_blocks(state, mature);
    }

    pub fn reclaim_all_blocks(&self) -> crate::heap::block_list::BlockList {
        let local_data = self.local_data_mut();
        let mut blocks = crate::heap::block_list::BlockList::new();

        for bucket in &mut local_data.allocator.young_generation {
            blocks.append(&mut bucket.blocks);
        }

        blocks.append(&mut local_data.allocator.mature_generation.blocks);

        blocks
    }

    pub fn set_main(&self) {
        self.local_data_mut().status.set_main();
    }

    pub fn is_main(&self) -> bool {
        self.local_data().status.is_main()
    }

    pub fn set_blocking(&self, enable: bool) {
        self.local_data_mut().status.set_blocking(enable);
    }

    pub fn is_blocking(&self) -> bool {
        self.local_data().status.is_blocking()
    }

    pub fn set_terminated(&self) {
        self.local_data_mut().status.set_terminated();
    }

    pub fn is_terminated(&self) -> bool {
        self.local_data().status.is_terminated()
    }

    pub fn thread_id(&self) -> Option<u8> {
        self.local_data().thread_id
    }

    pub fn set_thread_id(&self, id: u8) {
        self.local_data_mut().thread_id = Some(id);
    }

    pub fn unset_thread_id(&self) {
        self.local_data_mut().thread_id = None;
    }

    pub fn is_pinned(&self) -> bool {
        self.thread_id().is_some()
    }

    pub fn suspend_with_timeout(&self, timeout: Arc<Timeout>) {
        let pointer = Arc::into_raw(timeout);
        let tagged = crate::tagged_pointer::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn suspend_without_timeout(&self) {
        let pointer = ptr::null_mut();
        let tagged = crate::tagged_pointer::with_bit(pointer, SUSPENDED_BIT);

        self.suspended.atomic_store(tagged);
    }

    pub fn is_suspended_with_timeout(&self, timeout: &Arc<Timeout>) -> bool {
        let pointer = self.suspended.atomic_load();

        crate::tagged_pointer::untagged(pointer) == timeout.as_ptr()
    }

    /// Attempts to acquire the rights to reschedule this process.
    pub fn acquire_rescheduling_rights(&self) -> RescheduleRights {
        let current = self.suspended.atomic_load();

        if current.is_null() {
            RescheduleRights::Failed
        } else if self.suspended.compare_and_swap(current, ptr::null_mut()) {
            let untagged = crate::tagged_pointer::untagged(current);

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

use std::collections::VecDeque;

pub struct Mailbox {
    /// The messages stored in this mailbox.
    messages: VecDeque<ObjectPointer>,
}

impl Mailbox {
    pub fn new() -> Self {
        Mailbox {
            messages: VecDeque::new(),
        }
    }

    pub fn send(&mut self, message: ObjectPointer) {
        self.messages.push_back(message);
    }

    pub fn receive(&mut self) -> Option<ObjectPointer> {
        self.messages.pop_front()
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        for message in &self.messages {
            callback(message.pointer());
        }
    }

    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
}

use std::sync::atomic::AtomicU8;

/// The status of a process, represented as a set of bits.
///
/// We use an atomic U8 since an external process may read this value while we
/// are changing it (e.g. when a process sends a message while the receiver
/// enters the blocking status).
///
/// While concurrent reads are allowed, only the owning process should change
/// the status.
pub struct ProcessStatus {
    /// The bits used to indicate the status of the process.
    ///
    /// Multiple bits may be set in order to combine different statuses. For
    /// example, if the main process is blocking it will set both bits.
    bits: AtomicU8,
}

impl ProcessStatus {
    /// A regular process.
    const NORMAL: u8 = 0b0;

    /// The main process.
    const MAIN: u8 = 0b1;

    /// The process is performing a blocking operation.
    const BLOCKING: u8 = 0b10;

    /// The process is terminated.
    const TERMINATED: u8 = 0b100;

    pub fn new() -> Self {
        Self {
            bits: AtomicU8::new(Self::NORMAL),
        }
    }

    pub fn set_main(&mut self) {
        self.update_bits(Self::MAIN, true);
    }

    pub fn is_main(&self) -> bool {
        self.bit_is_set(Self::MAIN)
    }

    pub fn set_blocking(&mut self, enable: bool) {
        self.update_bits(Self::BLOCKING, enable);
    }

    pub fn is_blocking(&self) -> bool {
        self.bit_is_set(Self::BLOCKING)
    }

    pub fn set_terminated(&mut self) {
        self.update_bits(Self::TERMINATED, true);
    }

    pub fn is_terminated(&self) -> bool {
        self.bit_is_set(Self::TERMINATED)
    }

    fn update_bits(&mut self, mask: u8, enable: bool) {
        let bits = self.bits.load(Ordering::Acquire);
        let new_bits = if enable { bits | mask } else { bits & !mask };

        self.bits.store(new_bits, Ordering::Release);
    }

    fn bit_is_set(&self, bit: u8) -> bool {
        self.bits.load(Ordering::Acquire) & bit == bit
    }
}

impl RcProcess {
    /// Returns the unique identifier associated with this process.
    pub fn identifier(&self) -> usize {
        self.as_ptr() as usize
    }
}

impl PartialEq for RcProcess {
    fn eq(&self, other: &Self) -> bool {
        self.identifier() == other.identifier()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // This ensures the timeout is dropped if it's present, without having
        // to duplicate the dropping logic.
        self.acquire_rescheduling_rights();
    }
}
