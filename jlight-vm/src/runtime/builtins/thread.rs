use super::*;
use std::thread::spawn;

pub extern "C" fn thread_init(
    rt: &Runtime,
    this: ObjectPointer,
    args: &[ObjectPointer],
) -> Result<ObjectPointer, ObjectPointer> {
    assert!(args.len() >= 1);
    let function = args[0];
    if let ObjectValue::Function(ref f) = function.get().value {
        if !f.upvalues.is_empty() {
            return Err(rt.allocate_string(Arc::new(
                "Thread functions cannot capture variables".to_owned(),
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

    this.get_mut().value = ObjectValue::Thread(Some(th));
    Ok(this)
}

pub extern "C" fn thread_join(
    rt: &Runtime,
    this: ObjectPointer,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, ObjectPointer> {
    match this.get_mut().value {
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
        .add_attribute(&Arc::new(String::from("init")), init);
    state
        .thread_prototype
        .add_attribute(&Arc::new(String::from("join")), join);
}
