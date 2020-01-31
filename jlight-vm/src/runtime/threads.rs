use super::context::*;
use crate::util::arc::Arc;
use crate::util::ptr::Ptr;
use parking_lot::{Condvar, Mutex};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

thread_local! {
    pub static THREAD: Ptr<Arc<JThread>> = Ptr::new(JThread::new());
}

pub struct LocalData {
    pub context: Box<Context>,
}

pub struct JThread {
    pub local_data: Ptr<LocalData>,
}

impl JThread {
    pub fn new() -> Arc<JThread> {
        Arc::new(JThread {
            local_data: Ptr::new(LocalData {
                context: Box::new(Context::new()),
            }),
        })
    }
    pub fn local_data(&self) -> &LocalData {
        self.local_data.get()
    }

    pub fn local_data_mut(&self) -> &mut LocalData {
        self.local_data.get()
    }
    pub fn pop_context(&self) -> bool {
        let local_data = self.local_data_mut();
        if let Some(parent) = local_data.context.parent.take() {
            local_data.context = parent;
            false
        } else {
            true
        }
    }

    pub fn push_context(&self, context: Context) {
        let mut boxed = Box::new(context);
        let local_data = self.local_data_mut();
        let target = &mut local_data.context;

        std::mem::swap(target, &mut boxed);

        target.parent = Some(boxed);
    }

    pub fn context_mut(&self) -> &mut Context {
        &mut self.local_data_mut().context
    }

    pub fn context(&self) -> &Context {
        &self.local_data().context
    }

    pub fn each_pointer<F: FnMut(super::object::ObjectPointerPointer)>(&self, cb: F) {
        let ctx = self.context();
        ctx.each_pointer(cb);
    }
}

unsafe impl Sync for JThread {}
unsafe impl Send for JThread {}

pub struct Threads {
    pub threads: Mutex<Vec<Arc<JThread>>>,
    pub cond_join: Condvar,
}

impl Threads {
    pub fn new() -> Threads {
        Threads {
            threads: Mutex::new(Vec::new()),
            cond_join: Condvar::new(),
        }
    }

    pub fn attach_current_thread(&self) {
        THREAD.with(|thread| {
            let mut threads = self.threads.lock();
            threads.push(thread.get().clone());
        });
    }

    pub fn attach_thread(&self, thread: Arc<JThread>) {
        let mut threads = self.threads.lock();
        threads.push(thread);
    }

    pub fn detach_current_thread(&self) {
        THREAD.with(|thread| {
            let mut threads = self.threads.lock();
            threads.retain(|elem| !Arc::ptr_eq(elem, &*thread.get()));
            self.cond_join.notify_all();
        });
    }

    pub fn join_all(&self) {
        let mut threads = self.threads.lock();

        while threads.len() > 0 {
            self.cond_join.wait(&mut threads);
        }
    }

    pub fn each<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<JThread>),
    {
        let threads = self.threads.lock();

        for thread in threads.iter() {
            f(thread)
        }
    }
}
