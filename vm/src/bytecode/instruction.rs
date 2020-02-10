#[derive(Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(u8)]
pub enum Opcode {
    LoadNull,
    LoadUndefined,
    LoadInt,
    LoadNumber,
    LoadTrue,
    LoadFalse,
    LoadById,
    StoreById,
    LoadByValue,
    StoreByValue,
    LoadByIndex,
    StoreByIndex,
}
