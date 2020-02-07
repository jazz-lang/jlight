#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntType {
    I8,
    I16,
    I32,
    I64,
}
use crate::runtime::object::*;
use regalloc::Reg;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FusionInstruction {
    LoadBool(Reg, bool),
    AddI(IntType, Reg, Reg, Reg),
    AddF(Reg, Reg, Reg),
    SubI(IntType, Reg, Reg, Reg),
    SubF(Reg, Reg, Reg),
    DivI(IntType, Reg, Reg, Reg),
    DivF(Reg, Reg, Reg),
    MulI(IntType, Reg, Reg, Reg),
    MulF(Reg, Reg, Reg),
    ModI(IntType, Reg, Reg, Reg),
    ModF(Reg, Reg, Reg),
    ShlI(IntType, Reg, Reg, Reg),
    ShlF(Reg, Reg, Reg),
    ShrI(IntType, Reg, Reg, Reg),
    ShrF(Reg, Reg, Reg),
    Add(Reg, Reg, Reg),
    Div(Reg, Reg, Reg),
    Mul(Reg, Reg, Reg),
    Mod(Reg, Reg, Reg),
    Sub(Reg, Reg, Reg),
    Shl(Reg, Reg, Reg),

    Shr(Reg, Reg, Reg),
    Move(Reg, Reg),
    LoadNumber(Reg, u64),
    LoadField(Reg, Reg, Reg),
    StoreField(Reg, Reg, Reg),
    LoadThis(Reg),
    LoadU(Reg, u32),
    Safepoint,
    StoreU(Reg, u32),
    StoreStack(Reg, u32),
    LoadStack(Reg, u32),
    Call(Reg, Reg, u16),
    VirtCall(Reg, Reg, Reg, u16),
    Construct(Reg, Reg, u16),
    ConstructArray(Reg, Reg, u16),
    Goto(u16),
    GotoIfTrue(Reg, u16),
    GotoIfFalse(Reg, u16),
    ConditionalGoto(Reg, u16, u16),
    LoadStatic(Reg, ObjectPointer),
    LoadGlobal(Reg, ObjectPointer),
    Concat(Reg, Reg, Reg),
    CmpF(Cmp, Reg, Reg, Reg),
    CmpI(IntType, Reg, Reg, Reg),
    Cmp(Cmp, Reg, Reg, Reg),
    Push(Reg),
    Pop(Reg),
    GuardGreater(Reg, Reg),
    GuardLess(Reg, Reg),
    GuardGreaterEqual(Reg, Reg),
    GuardLessEqual(Reg, Reg),
    GuardEqual(Reg, Reg),
    GuardNotEqual(Reg, Reg),
    GuardNumber(Reg),
    GuardString(Reg),
    GuarArray(Reg),
    GuardInt8d(Reg),
    GuardInt16(Reg),
    GuardInt32(Reg),
    GuardInt64(Reg),
    GuardBool(Reg),
    GuardFunction(Reg),
    MakeEnv(Reg, u32),
    Return(Option<Reg>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Copy, Clone)]
pub enum Cmp {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FusionBasicBlock {
    pub instructions: Vec<FusionInstruction>,
}
