use crate::bytecode::*;
use crate::chunk::*;
use crate::object::*;
use crate::ptr::*;
use crate::sync::Arc;
use std::cell::UnsafeCell;
pub struct Context {
    pub registers: [ObjectPointer; 128],
    pub stack: Vec<ObjectPointer>,
    pub parent: Option<Box<Context>>,
    pub this: ObjectPointer,
    pub upvalues: Vec<ObjectPointer>,
    /// The index of the instruction to store prior to suspending a process.
    pub instruction_index: usize,
    pub block_index: usize,
    pub code: Vec<BasicBlock>,
    /// The register to store this context's return value in.
    pub return_register: Option<u16>,
    pub globals: Ptr<Vec<ObjectPointer>>,
    pub terminate_upon_return: bool,
}

impl Context {
    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        self.registers.iter().for_each(|x| callback(x.pointer()));
    }

    pub fn set_register(&mut self, r: u16, val: ObjectPointer) {
        self.registers[r as usize] = val;
    }

    pub fn get_register(&self, r: u16) -> ObjectPointer {
        self.registers[r as usize]
    }
}
