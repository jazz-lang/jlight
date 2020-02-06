use super::context::*;
use crate::util::ptr::Ptr;
use crate::util::shared::Arc;
use crate::util::shared::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct ThreadPtr(Ptr<Arc<JThread>>);

thread_local! {
    pub static THREAD:ThreadPtr = {
        let thread = JThread::new();
        thread.local_data_mut().native = true;
        ThreadPtr(Ptr::new(thread))
    };
}

impl std::ops::Deref for ThreadPtr {
    type Target = Ptr<Arc<JThread>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ThreadPtr {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw((self.0).0);
        }
    }
}

/// Thread local data in JLight.
pub struct LocalData {
    pub context: Ptr<Context>,
    pub native: bool,
}

/// JLight native thread.
///
/// Multiple instances of `JThread` may be used in single-thread.
pub struct JThread {
    pub local_data: Ptr<LocalData>,
}

impl JThread {
    pub fn new() -> Arc<JThread> {
        Arc::new(JThread {
            local_data: Ptr::new(LocalData {
                context: Ptr::new(Context::new()),
                native: false,
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
            let old = local_data.context;
            unsafe {
                std::ptr::drop_in_place(old.0);
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

    pub fn each_pointer<F: FnMut(super::object::ObjectPointerPointer)>(&self, cb: F) {
        let ctx = self.context();
        ctx.each_pointer(cb);
    }
}

unsafe impl Sync for JThread {}
unsafe impl Send for JThread {}

impl Drop for JThread {
    fn drop(&mut self) {
        let local_data = self.local_data();
        let mut context = Some(local_data.context);
        while let Some(ccontext) = context {
            let parent = ccontext.parent;
            unsafe {
                let _ = Box::from_raw(ccontext.0);
            }
            context = parent;
        }
        unsafe {
            let _ = Box::from_raw(self.local_data.0);
        }
    }
}

pub struct Threads {
    pub threads: Mutex<Vec<Arc<JThread>>>,
    pub cond_join: Condvar,
}

impl Threads {
    pub fn new() -> Threads {
        Threads {
            threads: Mutex::<Vec<Arc<JThread>>>::new(Vec::new()),
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

    pub fn detach_thread(&self, thread: Arc<JThread>) {
        let mut threads = self.threads.lock();
        threads.retain(|elem| !Arc::ptr_eq(elem, &thread));
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
