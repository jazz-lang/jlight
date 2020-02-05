use super::*;

pub extern "C" fn array_init(_: &Runtime, x: Value, args: &[Value]) -> Result<Value, Value> {
    match &mut x.as_cell().get_mut().value {
        ObjectValue::Array(ref mut array) => {
            for value in args.iter() {
                array.push(*value);
            }
        }
        x => {
            *x = ObjectValue::Array(args.to_vec());
        }
    }
    Ok(x)
}

pub extern "C" fn array_length(rt: &Runtime, x: Value, _: &[Value]) -> Result<Value, Value> {
    assert!(!x.is_null_or_undefined());
    match x.as_cell().get().value {
        ObjectValue::Array(ref x) => Ok(Value::new_double(x.len() as f64)),
        _ => Err(rt.allocate_string(Arc::new("Not an array".to_owned()))),
    }
}

pub(super) fn register_array(state: &mut RcState) {
    state.array_prototype.add_attribute(
        &Arc::new("init".to_owned()),
        new_native_fn(state, array_init, -1),
    );
    state.array_prototype.add_attribute(
        &Arc::new("length".to_owned()),
        new_native_fn(state, array_length, 0),
    );
}
