use super::threads::*;
use crate::heap::global::*;
use crate::heap::tracer::*;
use crate::runtime::object::*;
use crate::util::arc::Arc;
use ahash::AHashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub type RcState = Arc<State>;

pub struct State {
    pub threads: Threads,
    pub gc: GlobalCollector,
    pub nil_prototype: ObjectPointer,
    pub boolean_prototype: ObjectPointer,
    pub array_prototype: ObjectPointer,
    pub object_prototype: ObjectPointer,
    pub function_prototype: ObjectPointer,
    pub number_prototype: ObjectPointer,
    pub module_prototype: ObjectPointer,
    pub static_variables: AHashMap<String, ObjectPointer>,
}

impl State {
    pub fn new() -> Self {
        let gc = GlobalCollector {
            heap: Mutex::new(vec![]),
            threshold: AtomicUsize::new(4096),
            bytes_allocated: AtomicUsize::new(0),
            pool: Pool::new(num_cpus::get() / 2),
            collecting: AtomicBool::new(true),
        };
        let nil_prototype = gc.allocate(Object::new(ObjectValue::None));
        let boolean_prototype = gc.allocate(Object::new(ObjectValue::None));
        let array_prototype = gc.allocate(Object::new(ObjectValue::None));
        let object_prototype = gc.allocate(Object::new(ObjectValue::None));
        let function_prototype = gc.allocate(Object::new(ObjectValue::None));
        let number_prototype = gc.allocate(Object::new(ObjectValue::None));
        let module_prototype = gc.allocate(Object::new(ObjectValue::None));
        let map = map!(ahash
            "Object".to_owned() => object_prototype,
            "Boolean".to_owned() => boolean_prototype,
            "Number".to_owned() => number_prototype,
            "Function".to_owned() => function_prototype,
            "Module".to_owned() => module_prototype,
            "Array".to_owned() => array_prototype
        );
        Self {
            threads: Threads::new(),
            gc: gc,
            nil_prototype,
            boolean_prototype,
            array_prototype,
            function_prototype,
            object_prototype,
            number_prototype,
            module_prototype,
            static_variables: map,
        }
    }
}
