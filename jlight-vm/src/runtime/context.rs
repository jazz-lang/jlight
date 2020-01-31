use super::module::Module;
use super::object::*;
use super::state::*;
use crate::bytecode::block::BasicBlock;
use crate::bytecode::instructions::Instruction;
use crate::util::arc::Arc;
use crate::util::deref_ptr::DerefPointer;
use crate::util::ptr::*;

pub struct CatchEntry {
    pub register: u16,
    pub jump_to: u16,
}

pub struct Context {
    pub ip: usize,
    pub bp: usize,
    pub registers: [ObjectPointer; 48],
    /// Context stack, used for passing arguments and storing values if there are no enough registers
    pub stack: Vec<ObjectPointer>,
    pub upvalues: Vec<ObjectPointer>,
    pub this: ObjectPointer,
    pub return_register: Option<u32>,
    pub terminate_upon_return: bool,
    pub module: Arc<Module>,
    pub parent: Option<Box<Context>>,
}
use fxhash::FxBuildHasher;
use std::collections::HashMap;
impl Context {
    pub fn new() -> Self {
        Self {
            ip: 0,
            bp: 0,
            registers: [ObjectPointer::null(); 48],
            stack: vec![],
            upvalues: vec![],
            return_register: None,
            terminate_upon_return: true,
            module: Arc::new(Module {
                code: vec![],
                labels: HashMap::with_hasher(FxBuildHasher::default()),
                globals: Ptr::null(),
            }),
            parent: None,
            this: ObjectPointer::null(),
        }
    }

    pub fn each_pointer<F: FnMut(ObjectPointerPointer)>(&self, mut cb: F) {
        let mut current = Some(self);
        while let Some(context) = current {
            context
                .registers
                .iter()
                .for_each(|pointer| cb(pointer.pointer()));
            context
                .stack
                .iter()
                .for_each(|pointer| cb(pointer.pointer()));
            context
                .upvalues
                .iter()
                .for_each(|pointer| cb(pointer.pointer()));
            current = context.parent.as_ref().map(|c| &**c);
        }
    }

    pub fn fetch_ins(&mut self) -> Instruction {
        if self.ip >= self.module.code[self.bp].instructions.len() {
            self.bp += 1;
        }
        let ins: &Instruction = unsafe {
            self.module.code[self.bp]
                .instructions
                .get_unchecked(self.ip)
        };
        self.ip += 1;
        *ins
    }

    pub fn set_register(&mut self, r: u32, value: ObjectPointer) {
        self.registers[r as usize] = value;
    }

    pub fn get_register(&self, r: u32) -> ObjectPointer {
        self.registers[r as usize]
    }

    pub fn move_(&mut self, to: u32, from: u32) {
        self.registers[to as usize] = self.registers[from as usize];
    }

    pub fn swap_registers(&mut self, to: u32, from: u32) {
        let tmp = self.get_register(to);
        let from_v = self.get_register(from);
        self.set_register(to, from_v);
        self.set_register(to, tmp);
    }
}
