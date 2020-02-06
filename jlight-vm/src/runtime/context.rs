use super::module::Module;
use super::object::*;
use super::value::*;
use crate::bytecode::block::BasicBlock;
use crate::util::ptr::*;
use crate::util::shared::Arc;

pub struct CatchEntry {
    pub register: u16,
    pub jump_to: u16,
}

pub struct Context {
    pub ip: usize,
    pub bp: usize,
    pub registers: [Value; 48],
    /// Context stack, used for passing arguments and storing values if there are no enough registers
    pub stack: Vec<Value>,
    pub upvalues: Vec<Value>,
    pub this: Value,
    pub return_register: Option<u32>,
    pub terminate_upon_return: bool,
    pub module: Arc<Module>,
    pub code: Ptr<Vec<BasicBlock>>,
    pub parent: Option<Ptr<Context>>,
    pub function: Value,
}

impl Context {
    pub fn new() -> Self {
        Self {
            ip: 0,
            bp: 0,
            registers: [
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
                Value::from(VTag::Null),
            ],
            stack: vec![],
            upvalues: vec![],
            return_register: None,
            terminate_upon_return: false,
            module: Arc::new(Module {
                globals: Ptr::null(),
            }),
            code: Ptr::null(),
            parent: None,
            this: Value::from(VTag::Undefined),
            function: Value::from(VTag::Undefined),
        }
    }

    pub fn each_pointer<F: FnMut(ObjectPointerPointer)>(&self, mut cb: F) {
        let mut current = Some(self);
        while let Some(context) = current {
            context
                .registers
                .iter()
                .filter(|x| !x.is_null())
                .for_each(|pointer| cb(pointer.pointer()));
            context
                .stack
                .iter()
                .filter(|x| !x.is_null())
                .for_each(|pointer| cb(pointer.pointer()));
            context
                .upvalues
                .iter()
                .filter(|x| !x.is_null())
                .for_each(|pointer| cb(pointer.pointer()));
            if context.module.globals.is_null() == false {
                context
                    .module
                    .globals
                    .get()
                    .iter()
                    .for_each(|pointer| cb(pointer.pointer()));
            }
            current = context.parent.as_ref().map(|c| &**c);
        }
    }

    pub fn set_register(&mut self, r: u32, value: Value) {
        self.registers[r as usize] = value;
    }

    pub fn get_register(&self, r: u32) -> Value {
        self.registers[r as usize].clone()
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
