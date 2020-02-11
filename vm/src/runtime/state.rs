use super::cell::*;
use super::scheduler;
use super::value::*;
use crate::heap::PermanentHeap;
use crate::util::arc::Arc;
use parking_lot::Mutex;
use scheduler::process_scheduler::ProcessScheduler;
use scheduler::timeout_worker::TimeoutWorker;
pub struct State {
    pub scheduler: ProcessScheduler,
    pub timeout_worker: TimeoutWorker,
    pub perm_heap: Mutex<PermanentHeap>,
    pub string_prototype: Value,
    pub object_prototype: Value,
    pub array_prototype: Value,
    pub number_prototype: Value,
    pub function_prototype: Value,
    pub generator_prototype: Value,
    pub process_prototype: Value,
    pub module_prototype: Value,
    pub byte_array_prototype: Value,
}

#[inline]
fn nof_parallel_worker_threads(num: usize, den: usize, switch_pt: usize) -> usize {
    let ncpus = num_cpus::get();
    if ncpus <= switch_pt {
        if ncpus <= 1 {
            return 2;
        }
        ncpus
    } else {
        switch_pt + ((ncpus - switch_pt) * num) / den
    }
}

impl State {
    pub fn new() -> Arc<Self> {
        let mut perm = PermanentHeap::new(2 & 1024 * 1024);
        let object_prototype = perm.allocate_empty();
        let string_prototype = perm.allocate_empty();
        let array_prototype = perm.allocate_empty();
        let number_prototype = perm.allocate_empty();
        let function_prototype = perm.allocate_empty();
        let generator_prototype = perm.allocate_empty();
        let process_prototype = perm.allocate_empty();
        let module_prototype = perm.allocate_empty();
        /*
        Arc::new(Self {
            scheduler: ProcessScheduler::new(
                nof_parallel_worker_threads(5, 8, 8),
                nof_parallel_worker_threads(5, 8, 8),
            ),
            timeout_worker: TimeoutWorker::new(),

        })*/
        unimplemented!()
    }
}

pub type RcState = Arc<State>;
