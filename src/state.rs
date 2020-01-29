use crate::heap::global_allocator::RcGlobalAllocator;
use crate::object::*;
use crate::object_value::*;
use crate::sync::Arc;
use parking_lot::Mutex;

pub type RcState = Arc<State>;
pub struct State {
    pub global_allocator: RcGlobalAllocator,
    pub config: crate::config::Config,
    /// The prototype of the base object, used as the prototype for all other
    /// prototypes.
    pub object_prototype: ObjectPointer,

    /// The prototype for number objects.
    pub number_prototype: ObjectPointer,

    /// The prototype for string objects.
    pub string_prototype: ObjectPointer,

    /// The prototype for array objects.
    pub array_prototype: ObjectPointer,

    /// The prototype for booleans.
    pub boolean_prototype: ObjectPointer,

    /// The prototype for the "nil" object.
    pub nil_prototype: ObjectPointer,

    /// The singleton "nil" object.
    pub nil_object: ObjectPointer,

    /// The prototype for byte arrays.
    pub byte_array_prototype: ObjectPointer,

    /// The prototype to use for modules.
    pub module_prototype: ObjectPointer,

    /// The commandline arguments passed to an JLight program.
    pub arguments: Vec<ObjectPointer>,
    pub permanent_allocator: Mutex<crate::heap::permament::PermanentAllocator>,
    pub string_pool: Mutex<crate::string_pool::StringPool>,
    pub scheduler: crate::scheduler::ProcessScheduler,
}

macro_rules! intern_string {
    ($state:expr, $lookup:expr, $store:expr) => {{
        let mut pool = $state.string_pool.lock();

        if let Some(value) = pool.get($lookup) {
            return value;
        }

        let ptr = {
            let mut alloc = $state.permanent_allocator.lock();
            let value = ObjectValue::String(Arc::new($store));

            alloc.allocate_with_prototype(value, $state.string_prototype)
        };

        pool.add(ptr);

        ptr
    }};
}

impl State {
    /// Interns an owned String.
    pub fn intern_string(&self, string: String) -> ObjectPointer {
        intern_string!(self, &string, string)
    }
}
