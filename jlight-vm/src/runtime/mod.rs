pub mod builtins;
pub mod context;
pub mod interpreter;
pub mod module;
pub mod object;
pub mod state;
pub mod string_pool;
pub mod threads;

use module::*;
use state::RcState;
pub struct Runtime {
    pub state: RcState,
    pub registry: crate::util::arc::Arc<ModuleRegistry>,
}
