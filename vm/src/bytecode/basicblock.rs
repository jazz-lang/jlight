use super::instruction::*;
use std::vec::Vec;
pub struct BasicBlock {
    pub index: usize,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub instructions: Vec<Instruction>,
}

use core::hash::{Hash, Hasher};

impl Hash for BasicBlock {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
