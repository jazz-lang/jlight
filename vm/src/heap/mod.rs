pub mod gc;
pub mod space;
use crate::runtime::cell::*;
use crate::runtime::value::*;
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash)]
pub enum GCType {
    None,
    Young,
    Old,
}

/// Permanent heap.
///
/// Values that will not be collected and *must* be alive through entire program live should be allocated in perm heap.
pub struct PermanentHeap {
    pub space: space::Space,
}

impl PermanentHeap {
    pub fn new(perm_size: usize) -> Self {
        Self {
            space: space::Space::new(perm_size),
        }
    }
    pub fn allocate_empty(&mut self) -> Value {
        self.allocate(Cell::new(CellValue::None))
    }
    pub fn allocate(&mut self, cell: Cell) -> Value {
        let pointer = self
            .space
            .allocate(std::mem::size_of::<Cell>(), &mut false)
            .to_mut_ptr::<Cell>();
        unsafe {
            pointer.write(cell);
        }
        let mut cell = CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(pointer),
        };
        cell.set_permanent();
        Value::from(cell)
    }
}

pub struct Heap {
    pub new_space: space::Space,
    pub old_space: space::Space,
    pub needs_gc: GCType,
}

impl Heap {
    pub fn new(page_size: usize) -> Self {
        Self {
            new_space: space::Space::new(page_size),
            old_space: space::Space::new(page_size),
            needs_gc: GCType::None,
        }
    }

    pub fn copy_object(&mut self, object: Value) -> Value {
        if !object.is_cell() {
            return object;
        }

        let to_copy = object.as_cell();
        if to_copy.is_permanent() {
            return object;
        }
        let to_copy = to_copy.get();
        let value_copy = match &to_copy.value {
            CellValue::None => CellValue::None,
            CellValue::Number(x) => CellValue::Number(*x),
            CellValue::Bool(x) => CellValue::Bool(*x),
            CellValue::String(x) => CellValue::String(x.clone()),
            CellValue::Array(values) => {
                let new_values = values
                    .iter()
                    .map(|value| self.copy_object(*value))
                    .collect();
                CellValue::Array(new_values)
            }
            CellValue::Function(function) => {
                let name = function.name.clone();
                let argc = function.argc.clone();
                let module = function.module.clone();
                let upvalues = function
                    .upvalues
                    .iter()
                    .map(|x| self.copy_object(*x))
                    .collect();
                let native = function.native;
                let code = function.code.clone();
                CellValue::Function(Function {
                    name,
                    argc,
                    module,
                    upvalues,
                    native,
                    code,
                })
            }
            CellValue::ByteArray(array) => CellValue::ByteArray(array.clone()),
            CellValue::Module(module) => CellValue::Module(module.clone()),
        };
        let mut copy = if let Some(proto_ptr) = to_copy.prototype {
            let proto_copy = self.copy_object(Value::from(proto_ptr));
            Cell::with_prototype(value_copy, proto_copy.as_cell())
        } else {
            Cell::new(value_copy)
        };
        if let Some(map) = to_copy.attributes_map() {
            let mut map_copy = AttributesMap::with_capacity(map.len());
            for (key, val) in map.iter() {
                let key_copy = key.clone();
                let val = self.copy_object(*val);
                map_copy.insert(key_copy, val);
            }

            copy.set_attributes_map(map_copy);
        }

        Value::from(self.allocate(GCType::Young, copy))
    }

    pub fn allocate(&mut self, tenure: GCType, cell: Cell) -> CellPointer {
        assert_ne!(tenure, GCType::None);
        let space = if tenure == GCType::Old {
            &mut self.old_space
        } else {
            &mut self.new_space
        };
        let mut needs_gc = false;
        let result = space
            .allocate(std::mem::size_of::<Cell>(), &mut needs_gc)
            .to_mut_ptr::<Cell>();
        unsafe {
            result.write(cell);
        }
        self.needs_gc = if needs_gc { tenure } else { GCType::None };
        CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(result),
        }
    }
}
