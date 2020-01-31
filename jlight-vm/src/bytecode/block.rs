use super::instructions::Instruction;
#[derive(Clone,Debug)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
}
