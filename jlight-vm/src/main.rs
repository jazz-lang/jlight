use color_backtrace::{install_with_settings, Settings};
use jlight_vm::bytecode::framework::*;
use jlight_vm::bytecode::instructions::Instruction;
use regalloc::TypedIxVec;
use std::time;
fn main() {
    //install_with_settings(Settings::new());

    let mut func = BytecodeFunction::new();
    let r0 = func.new_virt_reg();
    let r1 = func.new_virt_reg();
    let r2 = func.new_virt_reg();
    let r3 = func.new_virt_reg();
    let r4 = func.new_virt_reg();
    func.block(
        0,
        TypedIxVec::from_vec(vec![
            Instruction::LoadInt(r0, 42),
            Instruction::LoadInt(r1, 3),
            Instruction::LoadInt(r4, 4),
            Instruction::Push(r0),
            Instruction::Add(r2, r0, r4),
            Instruction::Move(r3, r2),
            Instruction::Return(Some(r4)),
        ]),
    );
    println!("before regalloc:\n");
    for ins in func.instructions.iter() {
        println!("{:?}", ins);
    }
    regalloc(&mut func);
    println!("\nafter regalloc:\n");
    for ins in func.instructions.iter() {
        println!("{:?}", ins);
    }
}
