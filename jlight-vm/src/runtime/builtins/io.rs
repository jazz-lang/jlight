use super::*;

pub extern "C" fn io_writeln(_: &Runtime, _: Value, args: &[Value]) -> Result<Value, Value> {
    for (i, arg) in args.iter().enumerate() {
        print!("{}", arg.to_string());
        if i != args.len() - 1 {
            print!(" ");
        }
    }
    println!();
    Ok(Value::new_double(args.len() as f64))
}

pub extern "C" fn io_write(_: &Runtime, _: Value, args: &[Value]) -> Result<Value, Value> {
    for (i, arg) in args.iter().enumerate() {
        print!("{}", arg.to_string());
        if i != args.len() - 1 {
            print!(" ");
        }
    }
    Ok(Value::new_double(args.len() as f64))
}

pub(super) fn register_io(state: &mut RcState) {
    let io_object = state.gc.allocate(&**state, Object::new(ObjectValue::None));
    let writeln = Arc::new("writeln".to_owned());
    io_object.add_attribute(&**state, &writeln, new_native_fn(state, io_writeln, -1));
    io_object.add_attribute(
        &**state,
        &Arc::new(String::from("write")),
        new_native_fn(state, io_write, -1),
    );

    state.static_variables.insert("io".to_owned(), io_object);
}
