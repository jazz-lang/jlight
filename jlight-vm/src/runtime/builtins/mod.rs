pub use crate::runtime;
pub use crate::util::arc::Arc;
pub use runtime::object::*;
pub use runtime::state::*;
pub use runtime::value::*;
pub use runtime::Runtime;
pub use runtime::*;
pub mod array;
pub mod io;
pub mod thread;

pub extern "C" fn builtin_gc(rt: &Runtime, _: Value, _: &[Value]) -> Result<Value, Value> {
    rt.state.gc.collect(&rt.state);
    Ok(Value::new_double(0.0))
}

pub fn register_builtins(state: &mut RcState) {
    io::register_io(state);
    array::register_array(state);
    thread::register_thread(state);
    let f = new_native_fn(state, builtin_gc, 0);
    state.static_variables.insert("__gc".to_owned(), f);
}
