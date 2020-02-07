use super::*;
use crate::runtime::value::*;
use std::thread::spawn;
pub extern "C" fn thread_init(rt: &Runtime, this: Value, args: &[Value]) -> Result<Value, Value> {
    assert!(args.len() >= 1);
    let function = args[0];
    if let ObjectValue::Function(ref f) = function.as_cell().get().value {
        if !f.upvalues.is_empty() {
            return Err(rt.allocate_string(Arc::new(
                "Thread function cannot capture variables".to_owned(),
            )));
        }
    } else {
        return Err(rt.allocate_string(Arc::new("Not an function".to_owned())));
    }
    let th = std::thread::spawn(move || {
        RUNTIME.state.threads.attach_current_thread();
        let result = RUNTIME.run_function(function);
        RUNTIME.state.threads.detach_current_thread();
        result
    });

    this.as_cell().get_mut().value = ObjectValue::Thread(Some(th));
    Ok(this)
}

pub extern "C" fn thread_join(rt: &Runtime, this: Value, _: &[Value]) -> Result<Value, Value> {
    match this.as_cell().get_mut().value {
        ObjectValue::Thread(ref mut handle) => match handle.take() {
            Some(handle) => match handle.join() {
                Ok(value) => return Ok(value),
                Err(_) => {
                    return Err(rt.allocate_string(Arc::new("Failed to join thread".to_owned())))
                }
            },
            None => return Err(rt.allocate_string(Arc::new("Thread already joined".to_owned()))),
        },
        _ => unreachable!(),
    }
}

pub(super) fn register_thread(state: &mut RcState) {
    let init = new_native_fn(state, thread_init, 1);
    let join = new_native_fn(state, thread_join, 0);
    state
        .thread_prototype
        .add_attribute(&**state, &Arc::new(String::from("init")), init);
    state
        .thread_prototype
        .add_attribute(&**state, &Arc::new(String::from("join")), join);
}
