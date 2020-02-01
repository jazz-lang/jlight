#[derive(Clone, Debug)]
pub enum Instruction {
    Load(u32, u32, u32),
    Store(u32, u32, u32),
    LoadInt(u32, u64),
    LoadNull(u32),
    LoadBool(u32, bool),
    LoadNum(u32, u64),
    LoadConst(u32, u32),
    LoadGlobal(u32, u32),
    LoadThis(u32),
    LoadStatic(u32, u32),
    LoadU(u32, u32),
    StoreU(u32, u32),
    Construct(u32, u32, u32),
    ConstructArray(u32, u32),
    Call(u32, u32, u32),
    VirtCall(u32, u32, u32, u32),
    TailCall(u32, u32, u32),
    Return(Option<u32>),
    MakeEnv(u32, u32),
    Push(u32),
    Pop(u32),
    StoreStack(u32, u16),
    LoadStack(u32, u16),
    Goto(u16),
    GotoIfFalse(u32, u16),
    GotoIfTrue(u32, u16),
    ConditionalGoto(u32, u16, u16),
    Add(u32, u32, u32),
    Sub(u32, u32, u32),
    Div(u32, u32, u32),
    Mod(u32, u32, u32),
    Mul(u32, u32, u32),
    Shr(u32, u32, u32),
    Shl(u32, u32, u32),
    Greater(u32, u32, u32),
    Less(u32, u32, u32),
    GreaterEqual(u32, u32, u32),
    LessEqual(u32, u32, u32),
    Equal(u32, u32, u32),
    NotEqual(u32, u32, u32),
    Not(u32, u32),
    And(u32, u32, u32),
    Or(u32, u32, u32),
    Xor(u32, u32, u32),
    BoolAnd(u32, u32, u32),
    BoolOr(u32, u32, u32),

    CatchBlock(u32, u32),
    Move(u32, u32),
    Safepoint,
    UnfinishedGoto(String),
    UnfinishedGotoF(u32, String),
    UnfinishedGotoT(u32, String),
}
use regalloc::{
    BlockIx, InstIx, Map, MyRange, RealReg, RealRegUniverse, Reg, RegClass, Set, SpillSlot,
    TypedIxVec, VirtualReg, NUM_REG_CLASSES,
};

macro_rules! vreg {
    ($v: expr) => {
        Reg::new_virtual(RegClass::I64, $v as u32)
    };
}

impl Instruction {
    pub fn get_targets(&self) -> Vec<BlockIx> {
        match &self {
            Instruction::Goto(x)
            | Instruction::GotoIfFalse(_, x)
            | Instruction::GotoIfTrue(_, x) => vec![BlockIx::new(*x as u32)],
            Instruction::ConditionalGoto(_, x, y) => {
                vec![BlockIx::new(*x as u32), BlockIx::new(*y as u32)]
            }
            _ => vec![],
        }
    }

    pub fn get_reg_usage(&self, is_maped: bool) -> (Set<Reg>, Set<Reg>, Set<Reg>) {
        let mut def = Set::<Reg>::empty();
        let mut m0d = Set::<Reg>::empty();
        let mut uce = Set::<Reg>::empty();
        macro_rules! vreg {
            ($v: expr) => {
                if !is_maped {
                    Reg::new_virtual(RegClass::I64, $v as u32)
                } else {
                    Reg::new_real(RegClass::I64, 1, $v as _)
                }
            };
        }
        match self {
            Instruction::Load(x, y, z) => {
                def.insert(vreg!(*x));
                uce.insert(vreg!(*y));
                uce.insert(vreg!(*z));
            }
            Instruction::Store(x, y, z) => {
                m0d.insert(vreg!(*x));
                uce.insert(vreg!(*y));
                uce.insert(vreg!(*z));
            }
            Instruction::LoadNum(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadInt(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadConst(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadGlobal(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadNull(r) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadThis(r) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadBool(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadStatic(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::LoadU(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::StoreU(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::Construct(r0, r1, _) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
            }
            Instruction::ConstructArray(r0, _) => {
                def.insert(vreg!(*r0));
            }
            Instruction::Call(r0, r1, _) | Instruction::TailCall(r0, r1, _) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
            }
            Instruction::VirtCall(r0, r1, r2, _) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
                uce.insert(vreg!(*r2));
            }
            Instruction::Return(Some(r)) => {
                uce.insert(vreg!(*r));
            }
            Instruction::Return(None) => (),
            Instruction::MakeEnv(r0, _) => {
                m0d.insert(vreg!(*r0));
            }
            Instruction::Push(r0) => {
                uce.insert(vreg!(*r0));
            }
            Instruction::Pop(r0) => {
                def.insert(vreg!(*r0));
            }
            Instruction::Move(r0, r1) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
            }
            Instruction::StoreStack(r, _) => {
                uce.insert(vreg!(*r));
            }
            Instruction::LoadStack(r, _) => {
                def.insert(vreg!(*r));
            }
            Instruction::Add(r0, r1, r2)
            | Instruction::Sub(r0, r1, r2)
            | Instruction::Div(r0, r1, r2)
            | Instruction::Mod(r0, r1, r2)
            | Instruction::Mul(r0, r1, r2)
            | Instruction::Shr(r0, r1, r2)
            | Instruction::Shl(r0, r1, r2)
            | Instruction::Greater(r0, r1, r2)
            | Instruction::Less(r0, r1, r2)
            | Instruction::GreaterEqual(r0, r1, r2)
            | Instruction::LessEqual(r0, r1, r2)
            | Instruction::Equal(r0, r1, r2)
            | Instruction::NotEqual(r0, r1, r2)
            | Instruction::And(r0, r1, r2)
            | Instruction::Or(r0, r1, r2)
            | Instruction::Xor(r0, r1, r2)
            | Instruction::BoolAnd(r0, r1, r2)
            | Instruction::BoolOr(r0, r1, r2) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
                uce.insert(vreg!(*r2));
            }
            Instruction::Not(r0, r1) => {
                def.insert(vreg!(*r0));
                uce.insert(vreg!(*r1));
            }
            Instruction::GotoIfFalse(r0, _)
            | Instruction::GotoIfTrue(r0, _)
            | Instruction::ConditionalGoto(r0, _, _) => {
                uce.insert(vreg!(*r0));
            }
            _ => {}
        }

        (def, m0d, uce)
    }

    pub fn map_regs_d_u(
        &mut self,
        map_defs: &Map<VirtualReg, RealReg>,
        map_uses: &Map<VirtualReg, RealReg>,
    ) {
        macro_rules! map {
            (use $r: ident $($rest:tt)*) => {
                {
                    *$r = map_uses
                        .get(&vreg!(*$r).to_virtual_reg())
                        .unwrap()
                        .get_index() as u32;
                    map!($($rest)*);
                }
            };
            (def $r: ident $($rest:tt)*) => {
                {
                    *$r = map_defs
                        .get(&vreg!(*$r).to_virtual_reg())
                        .unwrap()
                        .get_index() as u32;
                    map!($($rest)*);
                }
            };
            () => {};
            ($($t: tt)*) => {
                map!($($t)*);
            }
        }
        match self {
            Instruction::LoadBool(r, _) => {
                map!(def r);
            }
            Instruction::Load(r0, r1, r2)
            | Instruction::Add(r0, r1, r2)
            | Instruction::Sub(r0, r1, r2)
            | Instruction::Div(r0, r1, r2)
            | Instruction::Mod(r0, r1, r2)
            | Instruction::Mul(r0, r1, r2)
            | Instruction::Shr(r0, r1, r2)
            | Instruction::Shl(r0, r1, r2)
            | Instruction::Greater(r0, r1, r2)
            | Instruction::Less(r0, r1, r2)
            | Instruction::GreaterEqual(r0, r1, r2)
            | Instruction::LessEqual(r0, r1, r2)
            | Instruction::Equal(r0, r1, r2)
            | Instruction::NotEqual(r0, r1, r2)
            | Instruction::And(r0, r1, r2)
            | Instruction::Or(r0, r1, r2)
            | Instruction::Xor(r0, r1, r2)
            | Instruction::BoolAnd(r0, r1, r2)
            | Instruction::BoolOr(r0, r1, r2) => {
                map!(def r0 use r1 use r2);
            }
            Instruction::LoadNull(r) => {
                map!(def r);
            }
            Instruction::Store(r0, r1, r2) => map!(use r0 use r1 use r2),
            Instruction::LoadNum(r0, _) => {
                map!(def r0);
            }
            Instruction::LoadInt(r0, _) => {
                map!(def r0);
            }
            Instruction::LoadConst(r0, _) => {
                map!(def r0);
            }
            Instruction::LoadGlobal(r0, _) => {
                map!(def r0);
            }
            Instruction::LoadThis(r0) => {
                map!(def r0);
            }
            Instruction::LoadStatic(r0, _) => {
                map!(def r0 );
            }
            Instruction::LoadU(r0, _) => {
                map!(def r0);
            }
            Instruction::StoreU(r0, _) => {
                map!(use r0);
            }
            Instruction::Construct(r0, r1, _) => {
                map!(def r0 use r1);
            }
            Instruction::ConstructArray(r0, _) => {
                map!(def r0);
            }
            Instruction::Call(r0, r1, _) | Instruction::TailCall(r0, r1, _) => {
                map!(def r0 use r1);
            }
            Instruction::VirtCall(r0, r1, r2, _) => map!(def r0 use r1 use r2),
            Instruction::Return(r0) => {
                if let Some(r0) = r0 {
                    map!(use r0);
                }
            }
            Instruction::MakeEnv(r0, _) => {
                map!(use r0);
            }
            Instruction::Push(r0) => {
                map!(use r0);
            }
            Instruction::Pop(r0) => {
                map!(def r0);
            }
            Instruction::Move(r0, r1) => {
                map!(def r0 use r1);
            }
            Instruction::StoreStack(r0, _) => {
                map!(use r0);
            }
            Instruction::LoadStack(r0, _) => {
                map!(def r0);
            }
            Instruction::Not(r0, r1) => map!(def r0 use r1),
            Instruction::GotoIfFalse(r0, _)
            | Instruction::GotoIfTrue(r0, _)
            | Instruction::ConditionalGoto(r0, _, _) => {
                map!(use r0);
            }

            _ => {}
        }
    }
}
