pub mod builtins;
pub mod context;
pub mod fusion;
//pub mod interpreter;
pub mod module;
pub mod object;
pub mod state;
pub mod string_pool;
pub mod threaded_interpreter;
pub mod threads;
pub mod value;
use crate::util::shared::Arc;
use module::*;
use object::*;
use state::*;
use threads::*;
use value::*;
pub struct Runtime {
    pub state: RcState,
    pub registry: crate::util::shared::Arc<ModuleRegistry>,
}

impl Drop for Runtime {
    fn drop(&mut self) {}
}

lazy_static::lazy_static! {
    pub static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new());
}

impl Runtime {
    pub fn new() -> Self {
        let mut state = Arc::new(State::new());
        builtins::register_builtins(&mut state);
        let registry = Arc::new(ModuleRegistry::new(state.clone()));
        Self { state, registry }
    }

    pub fn run_function(&self, function: Value) -> Value {
        if !function.is_cell() {
            panic!("not a function");
        }
        match function.as_cell().get().value {
            ObjectValue::Function(ref func) => threads::THREAD.with(|thread| {
                let mut context = context::Context::new();
                context.code = func.code.clone();
                context.module = func.module.clone();
                context.terminate_upon_return = true;
                context.function = function;
                context.upvalues = func.upvalues.clone();
                thread.get().push_context(context);
                unimplemented!()
                //return self.run(thread.get());
            }),
            _ => panic!("not a function"),
        }
    }

    pub fn run_function_with_thread(
        &self,
        function: ObjectPointer,
        thread: &mut Arc<JThread>,
    ) -> ObjectPointer {
        if function.is_tagged_number() {
            panic!("Not a function");
        }
        match function.get().value {
            ObjectValue::Function(_) => {
                unimplemented!()
                //return self.run(thread);
            }
            _ => panic!("not a function"),
        }
    }

    pub fn allocate_null(&self) -> Value {
        self.state.nil_prototype
    }

    pub fn allocate_string(&self, s: Arc<String>) -> Value {
        let object = Object::with_prototype(ObjectValue::String(s), self.state.string_prototype);
        self.state.gc.allocate(object)
    }

    pub fn allocate_bool(&self, x: bool) -> Value {
        let object = Object::with_prototype(ObjectValue::Bool(x), self.state.boolean_prototype);
        self.state.gc.allocate(object)
    }
}
