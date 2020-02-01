pub mod builtins;
pub mod context;
pub mod fusion;
pub mod interpreter;
pub mod module;
pub mod object;
pub mod state;
pub mod string_pool;
pub mod threads;

use crate::util::arc::Arc;
use module::*;
use object::*;
use state::*;
pub struct Runtime {
    pub state: RcState,
    pub registry: crate::util::arc::Arc<ModuleRegistry>,
}

impl Runtime {
    pub fn new() -> Self {
        let mut state = Arc::new(State::new());
        state.threads.attach_current_thread();
        builtins::register_builtins(&mut state);
        let registry = Arc::new(ModuleRegistry::new(state.clone()));
        Self { state, registry }
    }

    pub fn run_function(&self, function: ObjectPointer) {
        if function.is_tagged_number() {
            panic!("not a function");
        }
        match function.get().value {
            ObjectValue::Function(ref func) => threads::THREAD.with(|thread| {
                let mut context = context::Context::new();
                context.code = func.code.clone();
                context.module = func.module.clone();
                context.terminate_upon_return = true;
                context.upvalues = func.upvalues.clone();
                thread.get().push_context(context);
                self.run(thread.get());
            }),
            _ => panic!("not a function"),
        }
    }
}
