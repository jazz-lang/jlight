use super::instructions::Instruction;
#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
}

impl BasicBlock {
    pub fn join(&mut self, other: BasicBlock) {
        self.instructions.pop();
        for ins in other.instructions {
            self.instructions.push(ins);
        }
    }

    pub fn try_replace_branch_targets(&mut self, to: u16, from: u16) -> bool {
        if self.instructions.is_empty() {
            return false;
        }
        let last_ins_id = self.instructions.len() - 1;
        let last_ins = &mut self.instructions[last_ins_id];
        match *last_ins {
            Instruction::ConditionalGoto(r, if_true, if_false) => {
                if if_true == from || if_false == from {
                    let if_true = if if_true == from { to } else { if_true };
                    let if_false = if if_false == from { to } else { if_false };
                    *last_ins = Instruction::ConditionalGoto(r, if_true, if_false);
                    true
                } else {
                    false
                }
            }
            Instruction::Goto(t) => {
                if t == from {
                    *last_ins = Instruction::Goto(to);
                    true
                } else {
                    false
                }
            }
            Instruction::GotoIfFalse(r, t) => {
                if t == from {
                    *last_ins = Instruction::GotoIfFalse(r, to);
                    true
                } else {
                    false
                }
            }
            Instruction::GotoIfTrue(r, t) => {
                if t == from {
                    *last_ins = Instruction::GotoIfTrue(r, to);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn branch_targets(&self) -> (Option<u16>, Option<u16>) {
        if self.instructions.is_empty() {
            return (None, None);
        }

        let last_ins = &self.instructions[self.instructions.len() - 1];
        match *last_ins {
            Instruction::ConditionalGoto(_, if_true, if_false) => (Some(if_true), Some(if_false)),
            Instruction::Goto(t)
            | Instruction::GotoIfFalse(_, t)
            | Instruction::GotoIfTrue(_, t) => (Some(t), None),
            Instruction::Return(_) => (None, None),
            _ => panic!("Terminator not found"),
        }
    }
}
