pub mod builtins;
pub mod context;
pub mod fusion;
pub mod interpreter;
pub mod module;
pub mod object;
pub mod state;
pub mod string_pool;
pub mod threads;
pub mod threaded_interpreter;
pub mod value;
use crate::util::arc::Arc;
use module::*;
use object::*;
use state::*;
use threads::*;
pub struct Runtime {
    pub state: RcState,
    pub registry: crate::util::arc::Arc<ModuleRegistry>,
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

    pub fn run_function(&self, function: ObjectPointer) -> ObjectPointer {
        if function.is_tagged_number() {
            panic!("not a function");
        }
        match function.get().value {
            ObjectValue::Function(ref func) => threads::THREAD.with(|thread| {
                let mut context = context::Context::new();
                context.code = func.code.clone();
                context.module = func.module.clone();
                context.terminate_upon_return = true;
                context.function = function;
                context.upvalues = func.upvalues.clone();
                thread.get().push_context(context);
                return self.run(thread.get());
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
                return self.run(thread);
            }
            _ => panic!("not a function"),
        }
    }
    pub fn run_function_with_thread_and_tracing(
        &self,
        function: ObjectPointer,
        thread: &mut Arc<JThread>,
        trace_info: &mut std::collections::HashMap<
            ObjectPointer,
            fusion::tracing_interpreter::TraceInfo,
        >,
        args: &[ObjectPointer],
    ) -> (ObjectPointer, bool) {
        if function.is_tagged_number() {
            panic!("Not a function");
        }
        match function.get().value {
            ObjectValue::Function(ref func) => {
                let mut context = context::Context::new();
                context.code = func.code.clone();
                context.module = func.module.clone();
                context.terminate_upon_return = true;
                context.function = function;
                context.terminate_upon_return = true;
                context.stack = args.to_vec();
                //for (i, arg) in context.stack.iter().enumerate() {
                //println!("arg {} {}", i, arg.to_string());
                //}
                context.upvalues = func.upvalues.clone();
                thread.push_context(context);
                return self.run_tracing(thread, trace_info);
            }
            _ => panic!("not a function"),
        }
    }
}
