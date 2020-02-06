use super::threads::*;
use crate::heap::*;
use crate::runtime::object::*;
use crate::util::shared::*;
use ahash::AHashMap;

use std::sync::atomic::{AtomicBool, AtomicUsize};
pub type RcState = Arc<State>;
use super::value::Value;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub enum GCType {
    Parallel,
    Serial,
    Concurrent,
    Uninit,
}

static mut GC_TYPE: GCType = GCType::Serial;

pub fn init_gc(ty: GCType) {
    unsafe {
        if GC_TYPE == GCType::Uninit {
            GC_TYPE = ty;
        } else {
            panic!("GC Already initialized");
        }
    }
}

#[inline]
fn nof_parallel_worker_threads(num: usize, den: usize, switch_pt: usize) -> usize {
    let ncpus = num_cpus::get_physical();
    if ncpus <= switch_pt {
        if ncpus <= 1 {
            return 2;
        }
        ncpus
    } else {
        switch_pt + ((ncpus - switch_pt) * num) / den
    }
}

fn build_gc() -> Box<dyn GarbageCollector> {
    if unsafe { GC_TYPE == GCType::Uninit } {
        init_gc(GCType::Parallel);
    }
    match unsafe { GC_TYPE } {
        GCType::Parallel => {
            let workers = nof_parallel_worker_threads(5, 8, 8);
            Box::new(parallel::ParallelCollector::new(workers))
        }
        _ => unimplemented!(),
    }
}

pub struct State {
    pub threads: Threads,
    pub gc: Box<dyn GarbageCollector>,
    pub nil_prototype: Value,
    pub boolean_prototype: Value,
    pub array_prototype: Value,
    pub object_prototype: Value,
    pub function_prototype: Value,
    pub number_prototype: Value,
    pub module_prototype: Value,
    pub string_prototype: Value,
    pub thread_prototype: Value,
    pub static_variables: AHashMap<String, Value>,
}

impl State {
    pub fn new() -> Self {
        let gc = build_gc();
        let nil_prototype = gc.allocate(Object::new(ObjectValue::None));
        let boolean_prototype = gc.allocate(Object::new(ObjectValue::None));
        let array_prototype = gc.allocate(Object::new(ObjectValue::None));
        let object_prototype = gc.allocate(Object::new(ObjectValue::None));
        let function_prototype = gc.allocate(Object::new(ObjectValue::None));
        let number_prototype = gc.allocate(Object::new(ObjectValue::None));
        let module_prototype = gc.allocate(Object::new(ObjectValue::None));
        let string_prototype = gc.allocate(Object::new(ObjectValue::None));
        let thread_prototype = gc.allocate(Object::new(ObjectValue::None));
        let map = map!(ahash
            "Object".to_owned() => object_prototype,
            "Boolean".to_owned() => boolean_prototype,
            "Number".to_owned() => number_prototype,
            "Function".to_owned() => function_prototype,
            "Module".to_owned() => module_prototype,
            "Array".to_owned() => array_prototype,
            "String".to_owned() => string_prototype,
            "Thread".to_owned() => thread_prototype
        );
        Self {
            threads: Threads::new(),
            gc: gc,
            nil_prototype,
            boolean_prototype,
            array_prototype,
            thread_prototype,
            function_prototype,
            object_prototype,
            number_prototype,
            module_prototype,
            static_variables: map,
            string_prototype,
        }
    }
    pub fn each_pointer<F: FnMut(ObjectPointerPointer)>(&self, mut cb: F) {
        cb(self.nil_prototype.pointer());
        cb(self.boolean_prototype.pointer());
        cb(self.array_prototype.pointer());
        cb(self.function_prototype.pointer());
        cb(self.object_prototype.pointer());
        cb(self.number_prototype.pointer());
        cb(self.module_prototype.pointer());
        cb(self.string_prototype.pointer());
        for (_, var) in self.static_variables.iter() {
            cb(var.pointer());
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        /*self.nil_prototype.as_cell().finalize();
        self.boolean_prototype.as_cell().finalize();
        self.array_prototype.as_cell().finalize();
        self.function_prototype.as_cell().finalize();
        self.object_prototype.as_cell().finalize();
        self.number_prototype.as_cell().finalize();
        self.module_prototype.as_cell().finalize();
        self.string_prototype.as_cell().finalize();
        for (_, value) in self.static_variables.iter() {
            if value.is_cell() {
                value.as_cell().finalize();
            }
        }*/
        self.static_variables.clear();
    }
}
