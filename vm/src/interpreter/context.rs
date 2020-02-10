use crate::bytecode::basicblock::BasicBlock;
use crate::runtime;
use crate::util;
use runtime::cell::*;
use runtime::module::*;
use runtime::process::*;
use runtime::value::*;
use std::vec::Vec;
use util::arc::Arc;
use util::ptr::*;
pub struct Context {
    pub registers: [Value; 48],
    pub stack: Vec<Value>,
    pub module: Arc<Module>,
    pub parent: Option<Ptr<Context>>,
    pub index: usize,
    pub bindex: usize,
    pub code: Arc<Vec<BasicBlock>>,
    pub function: CellPointer,
}

impl Context {
    pub fn set_register(&mut self, r: u8, value: Value) {
        self.registers[r as usize] = value;
    }

    pub fn get_register(&self, r: u8) -> Value {
        self.registers[r as usize]
    }

    pub fn move_registers(&mut self, r0: u8, r1: u8) {
        let tmp = self.get_register(r0);
        self.registers[r0 as usize] = self.registers[r1 as usize];
        self.registers[r1 as usize] = tmp;
    }

    pub fn trace<F>(&self, mut cb: F)
    where
        F: FnMut(*const CellPointer),
    {
        let mut current = Some(self);
        while let Some(context) = current {
            context.registers.iter().for_each(|x| {
                if x.is_cell() {
                    unsafe { cb(&x.u.ptr) }
                }
            });

            context.stack.iter().for_each(|x| {
                if x.is_cell() {
                    unsafe { cb(&x.u.ptr) }
                }
            });
            context.module.globals.iter().for_each(|x| {
                if x.is_cell() {
                    unsafe { cb(&x.u.ptr) }
                }
            });
            cb(&context.function);
            current = context.parent.as_ref().map(|c| &**c);
        }
    }
}
