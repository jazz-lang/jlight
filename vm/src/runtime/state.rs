use super::cell::*;
use super::value::*;

pub struct State {
    pub string_prototype: CellPointer,
    pub object_prototype: CellPointer,
    pub array_prototype: CellPointer,
    pub number_prototype: CellPointer,
    pub function_prototype: CellPointer,
    pub generator_prototype: CellPointer,
    pub process_prototype: CellPointer,
}
