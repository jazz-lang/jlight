use crate::ast::*;
use block::*;
use framework::*;
use instructions::*;
use jlight_vm::bytecode::*;
use jlight_vm::runtime::module::*;
use jlight_vm::runtime::object::*;
use jlight_vm::util::arc::Arc;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub enum Global {
    Var(String),
    Func(i32, i32),
    Str(String),
    Float(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Access {
    Env(i32),
    Stack(i32),
    Global(i32),
    Field(Box<Expr>, String),
    Index(i32),
    Array(Box<Expr>, Box<Expr>),
    This,
}

pub struct Globals {
    pub globals: HashMap<Global, i32>,
    pub objects: HashMap<String, Vec<i32>>,
    pub functions: Vec<(Vec<BasicBlock>, Vec<(i32, i32)>, i32, i32)>,
    pub table: Vec<Global>,
}
use hashlink::*;

#[derive(Clone)]
pub struct LoopControlInfo {
    pub break_point: u16,
    pub continue_point: u16,
}

pub struct Context {
    pub g: Globals,
    pub bbs: Vec<BasicBlock>,
    pub current_bb: usize,
    pub locals: HashMap<String, i32>,
    pub env: HashMap<String, i32>,
    loop_control_info: Vec<LoopControlInfo>,
    pub stack: i32,
    pub limit: i32,
    pub nenv: i32,
    pub used_upvars: LinkedHashMap<String, i32>,
    pub pos: Vec<(i32, i32)>,
    pub cur_pos: (i32, i32),
    pub cur_file: String,
    pub regs: u32,
}

impl Context {
    pub fn get_current_bb(&mut self) -> &mut BasicBlock {
        &mut self.bbs[self.current_bb]
    }

    pub fn get_lci(&self) -> Option<&LoopControlInfo> {
        if !self.loop_control_info.is_empty() {
            Some(&self.loop_control_info[self.loop_control_info.len() - 1])
        } else {
            None
        }
    }
    pub fn new_reg(&mut self) -> u32 {
        let r = self.regs;
        self.regs += 1;
        r
    }
    pub fn move_forward(&mut self) {
        self.bbs.push(BasicBlock {
            instructions: vec![],
        });
        self.current_bb += 1;
    }

    pub fn write_break(&mut self) {
        let target = match self.get_lci() {
            Some(v) => v.break_point,
            None => panic!("Can't break"),
        };
        self.get_current_bb()
            .instructions
            .push(Instruction::Goto(target));
        self.move_forward();
    }

    pub fn write_continue(&mut self) {
        let target = match self.get_lci() {
            Some(v) => v.continue_point,
            None => panic!("can't continue"),
        };
        self.get_current_bb()
            .instructions
            .push(Instruction::Goto(target));
        self.move_forward();
    }

    pub fn with_lci<R, T: FnMut(&mut Self) -> R>(&mut self, lci: LoopControlInfo, mut f: T) -> R {
        self.loop_control_info.push(lci);
        let ret = catch_unwind(AssertUnwindSafe(|| f(self)));
        self.loop_control_info.pop().unwrap();

        match ret {
            Ok(v) => v,
            Err(e) => resume_unwind(e),
        }
    }

    pub fn scoped<R, T: FnMut(&mut Self) -> R>(&mut self, mut f: T) -> R {
        let prev = self.locals.clone();
        let ret = catch_unwind(AssertUnwindSafe(|| f(self)));
        self.locals = prev;

        match ret {
            Ok(v) => v,
            Err(e) => resume_unwind(e),
        }
    }
    pub fn global(&mut self, g: &Global) -> i32 {
        return match self.g.globals.get(g).cloned() {
            Some(g) => g.clone(),
            None => {
                let gid = self.g.table.len() as i32;
                self.g.globals.insert(g.clone(), gid);
                self.g.table.push(g.clone());
                gid
            }
        };
    }
    pub fn write(&mut self, ins: Instruction) {
        self.get_current_bb().instructions.push(ins);
    }
    pub fn access_set(&mut self, acc: Access, r: u32) {
        match acc {
            Access::Env(n) => {
                self.write(Instruction::StoreU(r, n as _));
            }
            Access::Stack(l) => self.write(Instruction::Move(l as _, r)),
            Access::Global(g) => unimplemented!(),
            Access::Field(obj, f) => {
                let gid = self.global(&Global::Str(f.to_owned()));
                let sr = self.new_reg();
                let obj = self.compile(&*obj, false);
                self.write(Instruction::LoadGlobal(sr, gid as _));
                self.write(Instruction::Store(obj, sr, r));
            }
            Access::Index(i) => unimplemented!(),
            //Access::This => self.write(Opcode::SetThis),
            //Access::Array => self.write(Opcode::SetArray),
            _ => unimplemented!(),
        }
    }
    pub fn access_get(&mut self, acc: Access) -> u32 {
        let r = self.new_reg();
        match acc {
            Access::Env(i) => {
                self.write(Instruction::LoadU(r, i as _));
                return r;
            }
            Access::Stack(l) => {
                self.write(Instruction::Move(r, l as _));
                return r;
            }
            Access::Global(x) => {
                self.write(Instruction::LoadGlobal(r, x as _));
                return r;
            }
            Access::Field(e, f) => {
                let gid = self.global(&Global::Str(f));
                let g = self.new_reg();
                self.write(Instruction::LoadGlobal(g, gid as _));
                let o = self.compile(&*e, false);
                self.write(Instruction::Load(r, o, g));
                return r;
            }
            Access::This => {
                self.write(Instruction::LoadThis(r));
                return r;
            }
            _ => unimplemented!(),
        }
    }
    pub fn compile_access(&mut self, e: &ExprKind) -> Access {
        match e {
            ExprKind::Ident(name) => {
                let l = self.locals.get(name);
                let s: &str = name;
                if l.is_some() {
                    let l = *l.unwrap();
                    return Access::Stack(l);
                } else if self.env.contains_key(s) {
                    let l = self.env.get(s);
                    self.used_upvars.insert(s.to_owned(), *l.unwrap());
                    self.nenv += 1;
                    return Access::Env(*l.unwrap());
                } else {
                    let g = self.global(&Global::Var(name.to_owned()));
                    return Access::Global(g);
                }
            }
            ExprKind::Access(e, f) => {
                return Access::Field(e.clone(), f.to_owned());
            }
            ExprKind::This => Access::This,
            ExprKind::ArrayIndex(ea, ei) => {
                return Access::Array(ea.clone(), ei.clone());
            }
            _ => unimplemented!(),
        }
    }

    pub fn load_args(&mut self, names: &[String]) {}

    pub fn compile(&mut self, e: &Expr, tail: bool) -> u32 {
        match &e.expr {
            ExprKind::Break => {
                self.write_break();
                0
            }
            ExprKind::Continue => {
                self.write_continue();
                0
            }
            ExprKind::Block(v) => {
                let last = self.scoped(|ctx| {
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Goto(expr_next_bb as _));
                    ctx.move_forward();
                    let mut last = None;
                    for x in v.iter() {
                        last = Some(ctx.compile(x, tail));
                    }
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Goto(expr_next_bb as _));
                    ctx.move_forward();
                    last
                });
                match last {
                    Some(r) => r,
                    _ => {
                        let x = self.new_reg();
                        self.write(Instruction::LoadNull(x));
                        x
                    }
                }
            }
            ExprKind::ConstInt(x) => {
                let r = self.new_reg();
                self.write(Instruction::LoadInt(r, *x as u64));
                r
            }
            ExprKind::ConstFloat(x) => {
                let r = self.new_reg();
                self.write(Instruction::LoadNum(r, x.to_bits()));
                r
            }
            ExprKind::ConstStr(s) => {
                let gid = self.global(&Global::Str(s.to_owned()));
                let g = self.new_reg();
                self.write(Instruction::LoadGlobal(g, gid as _));
                g
            }
            ExprKind::Return(e) => match e {
                Some(e) => {
                    let r = self.compile(e, false);
                    self.write(Instruction::Return(Some(r)));
                    r
                }
                _ => {
                    self.write(Instruction::Return(None));
                    0
                }
            },
            _ => unimplemented!(),
        }
    }
}
