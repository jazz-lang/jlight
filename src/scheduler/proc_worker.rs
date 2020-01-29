use super::state::*;
use super::worker::Worker;
use crate::process::*;
use crate::runtime::machine::*;
use crate::sync::*;
use num_bigint::BigInt;
use num_bigint::RandBigInt;
use queue::*;
use rand::distributions::uniform::{SampleBorrow, SampleUniform};
use rand::distributions::{Distribution, Standard};
use rand::rngs::ThreadRng;
use rand::{thread_rng, Rng};
use std::cell::UnsafeCell;
/// The state that a worker is in.
#[derive(Eq, PartialEq, Debug)]
pub enum Mode {
    /// The worker should process its own queue or other queues in a normal
    /// fashion.
    Normal,

    /// The worker should only process a particular job, and not steal any other
    /// jobs.
    Exclusive,
}

/// A worker owned by a thread, used for executing jobs from a scheduler queue.
pub struct ProcessWorker {
    /// The unique ID of this worker, used for pinning jobs.
    pub id: usize,

    /// A randomly generated integer that is incremented upon request. This can
    /// be used as a seed for hashing. The integer is incremented to ensure
    /// every seed is unique, without having to generate an entirely new random
    /// number.
    pub random_number: u64,

    /// The random number generator for this thread.
    pub rng: ThreadRng,

    /// The queue owned by this worker.
    queue: RcQueue<RcProcess>,

    /// The state of the pool this worker belongs to.
    state: Arc<PoolState<RcProcess>>,

    /// The Machine to use for running code.
    machine: UnsafeCell<Machine>,

    /// The mode this worker is in.
    mode: Mode,
}

impl ProcessWorker {
    pub fn new(
        id: usize,
        queue: RcQueue<RcProcess>,
        state: Arc<PoolState<RcProcess>>,
        machine: Machine,
    ) -> Self {
        ProcessWorker {
            id,
            random_number: rand::random(),
            rng: thread_rng(),
            queue,
            state,
            mode: Mode::Normal,
            machine: UnsafeCell::new(machine),
        }
    }

    /// Changes the worker state so it operates in exclusive mode.
    ///
    /// When in exclusive mode, only the currently running job will be allowed
    /// to run on this worker. All other jobs are pushed back into the global
    /// queue.
    pub fn enter_exclusive_mode(&mut self) {
        self.queue.move_external_jobs();

        while let Some(job) = self.queue.pop() {
            self.state.push_global(job);
        }

        self.mode = Mode::Exclusive;
    }

    pub fn leave_exclusive_mode(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn random_incremental_number(&mut self) -> u64 {
        self.random_number = self.random_number.wrapping_add(1);

        self.random_number
    }

    pub fn random_number<T>(&mut self) -> T
    where
        Standard: Distribution<T>,
    {
        self.rng.gen()
    }

    pub fn random_number_between<T: SampleUniform, V>(&mut self, min: V, max: V) -> T
    where
        V: SampleBorrow<T> + Sized,
    {
        self.rng.gen_range(min, max)
    }

    pub fn random_bigint_between(&mut self, min: &BigInt, max: &BigInt) -> BigInt {
        //self.rng.gen_bigint_range(min, max)
        unimplemented!()
    }

    pub fn random_bytes(&mut self, size: usize) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::with_capacity(size);

        unsafe {
            bytes.set_len(size);
        }

        self.rng
            .try_fill(&mut bytes[..])
            .map_err(|e| e.to_string())?;

        Ok(bytes)
    }

    /// Performs a single iteration of the normal work loop.
    fn normal_iteration(&mut self) {
        if self.process_local_jobs() {
            return;
        }

        if self.steal_from_other_queue() {
            return;
        }

        if self.queue.move_external_jobs() {
            return;
        }

        if self.steal_from_global_queue() {
            return;
        }

        self.state
            .park_while(|| !self.state.has_global_jobs() && !self.queue.has_external_jobs());
    }

    /// Runs a single iteration of an exclusive work loop.
    fn exclusive_iteration(&mut self) {
        if self.process_local_jobs() {
            return;
        }

        // Moving external jobs would allow other workers to steal them,
        // starving the current worker of pinned jobs. Since only one job can be
        // pinned to a worker, we don't need a loop here.
        if let Some(job) = self.queue.pop_external_job() {
            self.process_job(job);
            return;
        }

        self.state.park_while(|| !self.queue.has_external_jobs());
    }
}

impl Worker<RcProcess> for ProcessWorker {
    fn state(&self) -> &PoolState<RcProcess> {
        let x: &PoolState<RcProcess> = &self.state;
        x
    }

    fn queue(&self) -> &RcQueue<RcProcess> {
        &self.queue
    }

    fn run(&mut self) {
        while self.state.is_alive() {
            match self.mode {
                Mode::Normal => self.normal_iteration(),
                Mode::Exclusive => self.exclusive_iteration(),
            };
        }
    }

    fn process_job(&mut self, job: RcProcess) {
        // When using a Machine we need both an immutable reference to it (using
        // `self.machine`), and a mutable reference to pass as an argument.
        // Rust does not allow this, even though in this case it's perfectly
        // safe.
        //
        // To work around this we use UnsafeCell. We could use RefCell, but
        // since we know exactly how this code is used (it's only the lines
        // below that depend on this) the runtime reference counting is not
        // needed.
        let machine = unsafe { &*self.machine.get() };

        machine.run(self, &job);
    }
}
