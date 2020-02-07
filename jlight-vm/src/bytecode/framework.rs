use super::block::*;
use super::instructions::*;
use regalloc::Function as RFunction;

#[derive(Debug, Copy)]
pub struct Block {
    pub label: usize,
    pub start: InstIx,
    pub len: u32,
}
impl Block {
    pub fn new(label: usize, start: InstIx, len: u32) -> Self {
        Self { label, start, len }
    }
}
impl Clone for Block {
    // This is only needed for debug printing.
    fn clone(&self) -> Self {
        Block {
            label: self.label.clone(),
            start: self.start,
            len: self.len,
        }
    }
}
use crate::util::ptr::Ptr;
use std::collections::HashMap;
pub struct BytecodeFunction {
    pub blocks: TypedIxVec<BlockIx, Block>,
    pub instructions: TypedIxVec<InstIx, Instruction>,
    stack_positions: Ptr<HashMap<usize, usize>>,
    virtual_regs: usize,
    stack: Ptr<usize>,
}

use regalloc::{
    BlockIx, InstIx, Map, MyRange, RealReg, RealRegUniverse, Reg, RegClass, Set, SpillSlot,
    TypedIxVec, VirtualReg, NUM_REG_CLASSES,
};

impl BytecodeFunction {
    pub fn new() -> Self {
        Self {
            blocks: TypedIxVec::new(),
            instructions: TypedIxVec::new(),
            stack: Ptr::new(0),
            stack_positions: Ptr::new(HashMap::new()),
            virtual_regs: 0,
        }
    }
    pub fn new_virt_reg(&mut self) -> u32 {
        let v = self.virtual_regs;
        self.virtual_regs += 1;
        v as u32
    }

    pub fn spill(&self, x: SpillSlot) -> usize {
        let pos = x.get_usize();
        let real_pos = pos + *self.stack.get();
        self.stack_positions.get().insert(pos, real_pos);
        *self.stack.get() += 1;

        real_pos
    }
    pub fn to_basic_blocks(&self) -> Vec<BasicBlock> {
        let instructions = self.instructions.iter().collect::<Vec<_>>();
        let mut basic_blocks = vec![];
        for block in self.blocks.iter() {
            let insns = &instructions
                [block.start.get() as usize..block.start.get() as usize + block.len as usize];
            basic_blocks.push(BasicBlock {
                instructions: insns.iter().map(|x| (**x).clone()).collect(),
            });
        }
        basic_blocks
    }

    pub fn block(&mut self, idx: usize, mut insns: TypedIxVec<InstIx, Instruction>) {
        let start = self.instructions.len();
        let len = insns.len() as u32;
        self.instructions.append(&mut insns);
        let b = Block::new(idx, InstIx::new(start), len);
        self.blocks.push(b);
    }

    pub fn update_from_alloc(&mut self, result: regalloc::RegAllocResult<BytecodeFunction>) {
        self.instructions = TypedIxVec::from_vec(result.insns);
        let num_blocks = self.blocks.len();
        let mut i = 0;
        for bix in self.blocks.range() {
            let block = &mut self.blocks[bix];
            block.start = result.target_map[bix];
            block.len = if i + 1 < num_blocks {
                result.target_map[BlockIx::new(i + 1)].get()
            } else {
                self.instructions.len()
            } - block.start.get();
            i += 1;
        }
    }
}

static mut MAPED: bool = false;

impl regalloc::interface::Function for BytecodeFunction {
    type Inst = Instruction;

    fn is_ret(&self, ins: InstIx) -> bool {
        match &self.instructions[ins] {
            Instruction::Return(_) => true,
            _ => false,
        }
    }

    fn func_liveins(&self) -> Set<RealReg> {
        Set::empty()
    }

    fn func_liveouts(&self) -> Set<RealReg> {
        Set::empty()
    }

    fn insns(&self) -> &[Instruction] {
        self.instructions.elems()
    }

    fn insns_mut(&mut self) -> &mut [Instruction] {
        self.instructions.elems_mut()
    }

    fn get_insn(&self, ix: InstIx) -> &Instruction {
        &self.instructions[ix]
    }

    fn get_insn_mut(&mut self, ix: InstIx) -> &mut Instruction {
        &mut self.instructions[ix]
    }

    fn entry_block(&self) -> BlockIx {
        BlockIx::new(0)
    }
    fn blocks(&self) -> MyRange<BlockIx> {
        self.blocks.range()
    }
    /// Provide the range of instruction indices contained in each block.
    fn block_insns(&self, block: BlockIx) -> MyRange<InstIx> {
        MyRange::new(self.blocks[block].start, self.blocks[block].len as usize)
    }
    /// Get CFG successors: indexed by block, provide a list of successor blocks.
    fn block_succs(&self, block: BlockIx) -> Vec<BlockIx> {
        let last_insn = self.blocks[block].start.plus(self.blocks[block].len - 1);
        self.instructions[last_insn].get_targets()
    }
    /// Provide the defined, used, and modified registers for an instruction.
    fn get_regs(&self, insn: &Self::Inst) -> regalloc::InstRegUses {
        let (d, m, u) = insn.get_reg_usage(unsafe { MAPED });

        regalloc::InstRegUses {
            used: u,
            defined: d,
            modified: m,
        }
    }
    fn map_regs(
        insn: &mut Self::Inst,
        pre_map: &Map<VirtualReg, RealReg>,
        post_map: &Map<VirtualReg, RealReg>,
    ) {
        unsafe {
            MAPED = true;
        }
        insn.map_regs_d_u(
            /* define-map = */ post_map, /* use-map = */ pre_map,
        );
    }

    fn is_move(&self, insn: &Instruction) -> Option<(Reg, Reg)> {
        match insn {
            Instruction::Move(dst, src) => Some((
                Reg::new_virtual(RegClass::I64, *dst as _),
                Reg::new_virtual(RegClass::I64, *src as _),
            )),
            _ => None,
        }
    }

    fn get_spillslot_size(&self, _: RegClass, _: VirtualReg) -> u32 {
        // For our VM, every value occupies one spill slot.
        1
    }

    fn gen_spill(&self, to_slot: SpillSlot, from_reg: RealReg, _: VirtualReg) -> Instruction {
        //self.spill(to_slot);
        Instruction::Push(from_reg.get_index() as _)
    }

    fn gen_reload(&self, to: RealReg, slot: SpillSlot, _: VirtualReg) -> Instruction {
        Instruction::LoadStack(to.get_index() as u32, slot.get_usize() as u16)
    }

    fn gen_move(&self, to: RealReg, from: RealReg, _: VirtualReg) -> Instruction {
        Instruction::Move(to.get_index() as _, from.get_index() as _)
    }

    fn maybe_direct_reload(
        &self,
        _insn: &Self::Inst,
        _reg: VirtualReg,
        _slot: SpillSlot,
    ) -> Option<Instruction> {
        None
    }
}

fn make_universe() -> RealRegUniverse {
    const REG_COUNT: usize = 48;
    let mut regs = Vec::<(RealReg, String)>::new();
    let mut allocable_by_class = [None; NUM_REG_CLASSES];
    let mut index = 0u8;
    let first = index as usize;
    for i in 0..REG_COUNT {
        let name = format!("r{}", i).to_string();
        let reg = Reg::new_real(RegClass::I64, /*enc=*/ 0, index).to_real_reg();
        regs.push((reg, name));
        index += 1;
        let last = index as usize - 1;
        allocable_by_class[RegClass::I64.rc_to_usize()] = Some((first, last));
    }

    let allocable = regs.len();
    let univ = RealRegUniverse {
        regs,
        // all regs are allocable
        allocable,
        allocable_by_class,
    };
    univ.check_is_sane();

    univ
}

pub fn regalloc(f: &mut BytecodeFunction) {
    use regalloc::{allocate_registers, RegAllocAlgorithm};
    let algo = RegAllocAlgorithm::Backtracking;
    let result = allocate_registers(f, algo, &make_universe()).unwrap();
    f.update_from_alloc(result);
    unsafe {
        MAPED = false;
    }
}

impl Drop for BytecodeFunction {
    fn drop(&mut self) {
        unsafe {
            //let _ = Box::from_raw(self.stack_positions.0);
        }
    }
}
