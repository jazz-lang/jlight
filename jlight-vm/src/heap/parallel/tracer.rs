use crate::runtime::object::*;
use crate::util::shared::Arc;
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
pub struct Pool {
    /// A global queue to steal jobs from.
    global_queue: Injector<ObjectPointerPointer>,

    /// The list of queues we can steal work from.
    stealers: Vec<Stealer<ObjectPointerPointer>>,
    init_lock: std::sync::Once,
    //Option is fine as extra size goes from padding, so it
    //doesn't increase overall size, but when changing layout
    //consider to switch to MaybeUninit
    state: core::cell::Cell<Option<State>>,
    shutdown_num: usize,
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.shutdown();
        self.shutdown_num = 0;
    }
}

impl Pool {
    pub fn shutdown(&self) {
        let state = self.get_state();
        for _ in 0..self.shutdown_num {
            if state.send.send(Message::Shutdown).is_err() {
                panic!();
            }
        }
    }
    pub fn new(threads: usize) -> Arc<Pool> {
        let mut workers = Vec::new();
        let mut stealers = Vec::new();

        for _ in 0..threads {
            let worker = Worker::new_fifo();
            let stealer = worker.stealer();

            workers.push(worker);
            stealers.push(stealer);
        }
        let state = Arc::new(Self {
            global_queue: Injector::new(),
            stealers,
            init_lock: std::sync::Once::new(),
            state: std::cell::Cell::new(None),
            shutdown_num: threads,
        });
        let tracers = workers
            .into_iter()
            .map(|worker| Tracer::new(worker, state.clone()))
            .collect::<Vec<_>>();
        let start = std::time::Instant::now();
        tracers.into_iter().enumerate().for_each(|(idx, tracer)| {
            let recv = state.get_state().recv.clone();
            let send = state.get_state().msend.clone();
            std::thread::Builder::new()
                .name(format!("gc tracer-{}", idx))
                .spawn(move || loop {
                    match recv.recv() {
                        Ok(Message::Execute) => {
                            trace!(
                                "{}: execution message received",
                                std::thread::current().name().unwrap()
                            );
                            tracer.trace();
                            send.send(()).unwrap();
                        }
                        Ok(Message::Shutdown) => {
                            trace!("{}: shutdown", std::thread::current().name().unwrap());
                            break;
                        }
                        _ => panic!(),
                    }
                })
                .unwrap();
        });
        trace!(
            "Spawned {} GC worker threads in {}ns",
            threads,
            start.elapsed().as_nanos()
        );
        state
    }

    pub fn schedule(&self, pointer: ObjectPointerPointer) {
        self.global_queue.push(pointer);
    }

    fn get_state(&self) -> &State {
        self.init_lock.call_once(|| {
            let (send, recv) = crossbeam_channel::unbounded();
            let (msend, mrecv) = crossbeam_channel::unbounded();
            self.state.set(Some(State {
                send,
                recv,
                msend,
                mrecv,
            }))
        });

        match unsafe { &*self.state.as_ptr() } {
            Some(state) => state,
            None => unreachable!(),
        }
    }

    pub fn run(&self) {
        let state = self.get_state();
        for _ in 0..self.shutdown_num {
            let _ = state.send.send(Message::Execute);
        }

        for _ in 0..self.shutdown_num {
            state.mrecv.recv().unwrap();
        }
    }
}

unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

pub struct Tracer {
    queue: Worker<ObjectPointerPointer>,
    pool: Arc<Pool>,
}

impl Tracer {
    pub fn new(queue: Worker<ObjectPointerPointer>, pool: Arc<Pool>) -> Self {
        Self { queue, pool }
    }

    pub fn trace(&self) {
        while let Some(pointer_pointer) = self.pop_job() {
            let pointer = pointer_pointer.get();
            if pointer.is_null() {
                continue;
            }
            if pointer.is_tagged_number() {
                continue;
            }
            if pointer.get_color() == COLOR_BLACK {
                continue;
            }
            trace!(
                "{}: Tracing 0x{:p}",
                std::thread::current().name().unwrap(),
                pointer.raw.raw
            );
            pointer.set_color(COLOR_BLACK);
            self.schedule_child_pointers(*pointer);
        }
    }

    fn schedule_child_pointers(&self, pointer: ObjectPointer) {
        pointer.get().each_pointer(|child| {
            self.queue.push(child);
        });
    }

    fn pop_job(&self) -> Option<ObjectPointerPointer> {
        if let Some(job) = self.queue.pop() {
            return Some(job);
        }

        loop {
            match self.pool.global_queue.steal_batch_and_pop(&self.queue) {
                Steal::Retry => {}
                Steal::Empty => break,
                Steal::Success(job) => return Some(job),
            };
        }

        // We don't steal in random order, as we found that stealing in-order
        // performs better.
        for stealer in self.pool.stealers.iter() {
            loop {
                match stealer.steal_batch_and_pop(&self.queue) {
                    Steal::Retry => {}
                    Steal::Empty => break,
                    Steal::Success(job) => return Some(job),
                }
            }
        }

        None
    }
}

enum Message {
    Execute,
    Shutdown,
}

struct State {
    send: crossbeam_channel::Sender<Message>,
    recv: crossbeam_channel::Receiver<Message>,
    msend: crossbeam_channel::Sender<()>,
    mrecv: crossbeam_channel::Receiver<()>,
}
