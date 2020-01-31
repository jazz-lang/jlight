pub use crate::runtime;
pub use crate::util::arc::Arc;
pub use runtime::object::*;
pub use runtime::state::*;
pub use runtime::Runtime;
pub mod io;

pub fn register_builtins(state: &mut RcState) {
    io::register_io(state);
}
