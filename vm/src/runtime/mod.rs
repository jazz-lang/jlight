pub mod cell;
pub mod channel;
pub mod config;
pub mod module;
pub mod process;
pub mod process_functions;
pub mod scheduler;
pub mod state;
pub mod value;

lazy_static::lazy_static!(
    static ref RUNTIME: Runtime = Runtime::new();
);

pub struct Runtime {}

impl Runtime {
    pub fn new() -> Self {
        Self {}
    }
}
