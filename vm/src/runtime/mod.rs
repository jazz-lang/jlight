pub mod cell;
pub mod channel;
pub mod config;
pub mod module;
pub mod process;
pub mod process_functions;
pub mod scheduler;
pub mod state;
pub mod value;

use state::*;

lazy_static::lazy_static!(
    static ref RUNTIME: Runtime = Runtime::new();
);

pub struct Runtime {
    pub state: RcState,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            state: State::new(config::Config::default()),
        }
    }
}
