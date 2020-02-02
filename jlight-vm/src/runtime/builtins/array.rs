use super::*;

pub extern "C" fn array_init(
    _: &Runtime,
    x: ObjectPointer,
    args: &[ObjectPointer],
) -> Result<ObjectPointer, ObjectPointer> {
    match &mut x.get_mut().value {
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

pub extern "C" fn array_length(
    rt: &Runtime,
    x: ObjectPointer,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, ObjectPointer> {
    assert!(!x.is_null());
    match x.get().value {
        ObjectValue::Array(ref x) => Ok(ObjectPointer::number(x.len() as f64)),
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
