use super::proc_worker::*;
use super::state::*;
use super::worker::*;
use crate::process::RcProcess;
use crate::runtime::machine::Machine;
use crate::sync::join_list::JoinList;
use crate::sync::queue::RcQueue;
use crate::sync::Arc;
use std::thread;

/// A pool of threads for running lightweight processes.
///
/// A pool consists out of one or more workers, each backed by an OS thread.
/// Workers can perform work on their own as well as steal work from other
/// workers.
pub struct ProcessPool {
    pub state: Arc<PoolState<RcProcess>>,

    /// The base name of every thread in this pool.
    name: String,
}

impl ProcessPool {
    pub fn new(name: String, threads: usize) -> Self {
        assert!(
            threads > 0,
            "A ProcessPool requires at least a single thread"
        );

        Self {
            name,
            state: Arc::new(PoolState::new(threads)),
        }
    }

    /// Schedules a job onto a specific queue.
    pub fn schedule_onto_queue(&self, queue: usize, job: RcProcess) {
        self.state.schedule_onto_queue(queue, job);
    }

    /// Schedules a job onto the global queue.
    pub fn schedule(&self, job: RcProcess) {
        self.state.push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    pub fn terminate(&self) {
        self.state.terminate();
    }

    /// Starts the pool, blocking the current thread until the pool is
    /// terminated.
    ///
    /// The current thread will be used to perform jobs scheduled onto the first
    /// queue.
    pub fn start_main(&self, machine: Machine) -> JoinList<()> {
        let join_list = self.spawn_threads_for_range(1, machine.clone());
        let queue = self.state.queues[0].clone();

        ProcessWorker::new(0, queue, self.state.clone(), machine).run();

        join_list
    }

    /// Starts the pool, without blocking the calling thread.
    pub fn start(&self, machine: Machine) -> JoinList<()> {
        self.spawn_threads_for_range(0, machine)
    }

    /// Spawns OS threads for a range of queues, starting at the given position.
    fn spawn_threads_for_range(&self, start_at: usize, machine: Machine) -> JoinList<()> {
        let mut handles = Vec::new();

        for index in start_at..self.state.queues.len() {
            let handle =
                self.spawn_thread(index, machine.clone(), self.state.queues[index].clone());

            handles.push(handle);
        }

        JoinList::new(handles)
    }

    fn spawn_thread(
        &self,
        id: usize,
        machine: Machine,
        queue: RcQueue<RcProcess>,
    ) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("{}-{}", self.name, id))
            .spawn(move || {
                ProcessWorker::new(id, queue, state, machine).run();
            })
            .unwrap()
    }
}
