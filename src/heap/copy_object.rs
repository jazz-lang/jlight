use crate::{object::*, object_value::*};

pub trait CopyObject: Sized {
    /// Allocates a copied object.
    fn allocate_copy(&mut self, _: Object) -> ObjectPointer;

    /// Performs a deep copy of the given pointer.
    ///
    /// The copy of the input object is allocated on the current heap.
    fn copy_object(&mut self, to_copy_ptr: ObjectPointer) -> ObjectPointer {
        if to_copy_ptr.is_permanent() {
            return to_copy_ptr;
        }

        let to_copy = to_copy_ptr.get();

        // Copy over the object value
        let value_copy = match to_copy.value {
            ObjectValue::None => ObjectValue::None,
            ObjectValue::Number(x) => ObjectValue::Number(x),
            ObjectValue::BigInt(ref bigint) => ObjectValue::BigInt(bigint.clone()),
            ObjectValue::String(ref string) => ObjectValue::String(string.clone()),
            ObjectValue::Array(ref raw_vec) => {
                let new_map = raw_vec.iter().map(|val_ptr| self.copy_object(*val_ptr));

                ObjectValue::Array(Box::new(new_map.collect::<Vec<_>>()))
            }
            ObjectValue::Hasher(ref h) => ObjectValue::Hasher(h.clone()),
            ObjectValue::File(_) => {
                panic!("ObjectValue::File can not be cloned");
            }
            ObjectValue::ByteArray(ref byte_array) => ObjectValue::ByteArray(byte_array.clone()),

            ObjectValue::Function(ref val) => {
                let upvalues = val
                    .upvalues
                    .iter()
                    .map(|x| self.copy_object(*x))
                    .collect::<Vec<_>>();

                ObjectValue::Function(Box::new(Function {
                    upvalues,
                    name: val.name.clone(),
                    block: val.block,
                    native: val.native,
                }))
            }
            ObjectValue::Bool(x) => ObjectValue::Bool(x),
            ObjectValue::Process(ref x) => ObjectValue::Process(x.clone()),
            ObjectValue::Module(ref module) => ObjectValue::Module(module.clone()),
        };

        let mut copy = if let Some(proto_ptr) = to_copy.prototype() {
            let proto_copy = self.copy_object(proto_ptr);

            Object::with_prototype(value_copy, proto_copy)
        } else {
            Object::new(value_copy)
        };

        if let Some(map) = to_copy.attributes_map() {
            let mut map_copy = AttributesMap::default();

            for (key, val) in map.iter() {
                let key_copy = self.copy_object(*key);
                let val_copy = self.copy_object(*val);

                map_copy.insert(key_copy, val_copy);
            }

            copy.set_attributes_map(map_copy);
        }

        self.allocate_copy(copy)
    }
}
