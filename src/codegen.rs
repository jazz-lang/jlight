/*
*   Copyright (c) 2020 Adel Prokurov
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/
use crate::ast::*;
use basicblock::*;
use cell::*;
use hashlink::LinkedHashMap;
use module::*;
use runtime::*;
use value::*;
use waffle::runtime;

use instruction::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::rc::Rc;
use waffle::bytecode::*;
use waffle::util::arc::Arc;
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
    Stack(String, i32),
    Global(i32, bool, String),
    Field(Box<Expr>, String),
    Index(i32),
    Array(Box<Expr>, Box<Expr>),
    This,
}

pub struct Globals {
    pub globals: LinkedHashMap<Global, i32>,
    pub objects: LinkedHashMap<String, Vec<i32>>,
    pub functions: Vec<(Vec<BasicBlock>, Vec<(i32, i32)>, i32, i32, String)>,
    pub table: Vec<Global>,
}

#[derive(Clone)]
pub struct LoopControlInfo {
    pub break_point: u16,
    pub continue_point: u16,
}

pub struct Context {
    pub g: Rc<RefCell<Globals>>,
    pub bbs: Vec<BasicBlock>,
    pub current_bb: usize,
    pub locals: LinkedHashMap<String, i32>,
    pub env: LinkedHashMap<String, i32>,
    pub labels: HashMap<String, Option<u32>>,
    loop_control_info: Vec<LoopControlInfo>,
    pub stack: i32,
    pub limit: i32,
    pub nenv: i32,
    pub used_upvars: LinkedHashMap<String, i32>,
    pub pos: Vec<(i32, i32)>,
    pub cur_pos: (i32, i32),
    pub cur_file: String,
    pub regs: u16,
}

impl Context {
    pub fn new_empty_label(&mut self) -> String {
        let lab_name = self.labels.len().to_string();
        self.labels.insert(lab_name.clone(), None);
        lab_name
    }
    pub fn label_here(&mut self, label: &str) {
        *self.labels.get_mut(label).unwrap() = Some(self.current_bb as _);
    }
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
    pub fn new_reg(&mut self) -> u16 {
        let r = self.regs;
        self.regs += 1;
        r
    }
    pub fn move_forward(&mut self) {
        let id = self.bbs.len();
        self.bbs.push(BasicBlock {
            instructions: vec![],
            index: id,
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
            .push(Instruction::Branch(target));
        self.move_forward();
    }

    pub fn write_continue(&mut self) {
        let target = match self.get_lci() {
            Some(v) => v.continue_point,
            None => panic!("can't continue"),
        };
        self.get_current_bb()
            .instructions
            .push(Instruction::Branch(target));
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
    pub fn global(&mut self, global: &Global) -> (i32, bool) {
        let mut g = self.g.borrow_mut();
        return match g.globals.get(global).cloned() {
            Some(g) => (g.clone(), false),
            None => {
                let gid = g.table.len() as i32;
                g.globals.insert(global.clone(), gid);
                g.table.push(global.clone());
                (gid, true)
            }
        };
    }

    pub fn global2(&mut self, global: &Global) -> (i32, bool) {
        let g = self.g.borrow_mut();
        return match g.globals.get(global).cloned() {
            Some(g) => (g.clone(), false),
            None => {
                let gid = g.table.len() as i32;
                (gid, true)
            }
        };
    }
    pub fn write(&mut self, ins: Instruction) {
        self.get_current_bb().instructions.push(ins);
    }
    pub fn access_set(&mut self, acc: Access, r: u16) -> u16 {
        match acc {
            Access::Env(n) => {
                self.write(Instruction::StoreUpvalue(r, n as _));
                return r;
            }
            Access::Stack(name, l) => {
                //let l = self.new_reg();
                self.write(Instruction::Move(l as _, r));
                //self.locals.insert(name, l as _);
                return r as _;
            }
            Access::Global(_, _, _) => unimplemented!(),
            Access::Field(obj, f) => {
                let (gid, _) = self.global(&Global::Str(f.to_owned()));
                let obj = self.compile(&*obj, false);
                //self.write(Instruction::LoadConst(sr, gid as _));
                self.write(Instruction::StoreById(obj, r, gid as _));
                return r;
            }
            Access::Index(_) => unimplemented!(),
            Access::Array(value, index) => {
                let value = self.compile(&value, false);
                let index = self.compile(&index, false);
                self.write(Instruction::StoreByValue(value, index, r));
                return r;
            }
            //Access::This => self.write(Opcode::SetThis),
            //Access::Array => self.write(Opcode::SetArray),
            _ => unimplemented!(),
        }
    }
    pub fn access_get(&mut self, acc: Access) -> u16 {
        let r = self.new_reg();
        match acc {
            Access::Env(i) => {
                self.write(Instruction::LoadUpvalue(r, i as _));
                return r;
            }
            Access::Stack(_, l) => {
                self.write(Instruction::Move(r, l as _));
                return r;
            }
            Access::Global(x, n, name) => {
                if !n {
                    self.write(Instruction::LoadConst(r, x as _));
                } else {
                    let (gid, _) = self.global(&Global::Str(name));
                    self.write(Instruction::LoadStaticById(r, gid as _));
                }
                return r;
            }
            Access::Field(e, f) => {
                let (gid, _) = self.global(&Global::Str(f));
                //let g = self.new_reg();
                //self.write(Instruction::LoadConst(g as _, gid as _));
                let o = self.compile(&*e, false);
                self.write(Instruction::LoadById(r as _, o as _, gid as _));
                return r;
            }
            Access::This => {
                self.write(Instruction::LoadThis(r));
                return r;
            }
            Access::Array(value, index) => {
                let value = self.compile(&value, false);
                let index = self.compile(&index, false);
                self.write(Instruction::LoadByValue(r, value, index));
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
                    return Access::Stack(name.to_owned(), l);
                } else if self.env.contains_key(s) {
                    let l = self.env.get(s);
                    self.used_upvars.insert(s.to_owned(), *l.unwrap());
                    self.nenv += 1;
                    return Access::Env(*l.unwrap());
                } else {
                    let (g, n) = self.global(&Global::Var(name.to_owned()));
                    return Access::Global(g, n, name.to_owned());
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
    pub fn compile_binop(&mut self, op: &str, e1: &Expr, e2: &Expr, tail: bool) -> u16 {
        match op {
            "==" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Equal, r3, r1, r2));
                r3
            }
            "!=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::NotEqual, r3, r1, r2));
                r3
            }
            ">" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Greater, r3, r1, r2));
                r3
            }
            ">=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::GreaterOrEqual, r3, r1, r2));
                r3
            }
            "<" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Less, r3, r1, r2));
                r3
            }
            "<=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::LessOrEqual, r3, r1, r2));
                r3
            }
            "+" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Add, r3, r1, r2));
                r3
            }
            "-" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Sub, r3, r1, r2));
                r3
            }
            "/" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Div, r3, r1, r2));
                r3
            }
            "*" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Mul, r3, r1, r2));
                r3
            }
            _ => unimplemented!(),
        }
    }

    pub fn compile(&mut self, e: &Expr, tail: bool) -> u16 {
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
                if v.is_empty() {
                    let r = self.new_reg();
                    self.write(Instruction::LoadNull(r));
                    return r;
                }
                let last = self.scoped(|ctx| {
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Branch(expr_next_bb as _));
                    ctx.move_forward();
                    let mut last = None;
                    for x in v.iter() {
                        last = Some(ctx.compile(x, tail));
                    }
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Branch(expr_next_bb as _));
                    ctx.move_forward();
                    last
                });
                match last {
                    Some(r) if r != 0 => r,
                    _ => {
                        let x = self.new_reg();
                        self.write(Instruction::LoadNull(x));
                        x
                    }
                }
            }
            ExprKind::BinOp(e1, op, e2) => self.compile_binop(op, e1, e2, tail),
            ExprKind::ConstInt(x) => {
                let r = self.new_reg();
                self.write(Instruction::LoadInt(r, *x as i32));
                r
            }
            ExprKind::ConstFloat(x) => {
                let r = self.new_reg();
                self.write(Instruction::LoadNumber(r, x.to_bits()));
                r
            }
            ExprKind::ConstStr(s) => {
                let (gid, _) = self.global(&Global::Str(s.to_owned()));
                let g = self.new_reg();
                self.write(Instruction::LoadConst(g, gid as _));
                g
            }
            ExprKind::Return(e) => match e {
                Some(e) => {
                    let r = self.compile(e, true);
                    self.write(Instruction::Return(Some(r)));
                    self.move_forward();
                    r
                }
                _ => {
                    self.write(Instruction::Return(None));
                    self.move_forward();
                    0
                }
            },

            ExprKind::Var(_, name, init) => {
                let r = match init {
                    Some(val) => {
                        let x = self.compile(&**val, tail);
                        let r = self.new_reg();
                        self.write(Instruction::Move(r, x));
                        r
                    }
                    _ => {
                        let r = self.new_reg();
                        self.write(Instruction::LoadNull(r));
                        r
                    }
                };
                self.locals.insert(name.to_owned(), r as _);

                r
            }
            ExprKind::Assign(lhs, rhs) => {
                let a = self.compile_access(&lhs.expr);
                let r = self.compile(rhs, false);
                self.access_set(a, r)
            }
            ExprKind::If(cond, if_true, if_false) => {
                let before_bb_id = self.current_bb;
                self.move_forward();
                let terminator_bb_id = self.current_bb;
                self.move_forward();
                let r = self.compile(cond, tail);
                let br_begin = self.current_bb;
                let else_begin;
                if let Some(if_false) = if_false {
                    self.move_forward();
                    else_begin = self.current_bb;
                    let _last = self.compile(if_false, tail);
                    //self.write(Instruction::Move(ret, last));
                    self.get_current_bb()
                        .instructions
                        .push(Instruction::Branch(terminator_bb_id as _));
                } else {
                    else_begin = terminator_bb_id;
                }
                self.move_forward();
                self.bbs[before_bb_id]
                    .instructions
                    .push(Instruction::Branch(br_begin as _));
                let checker_bb = br_begin;
                let current_bb = self.current_bb;
                self.bbs[checker_bb]
                    .instructions
                    .push(Instruction::ConditionalBranch(
                        r,
                        current_bb as _,
                        else_begin as _,
                    ));
                self.compile(if_true, tail);
                self.write(Instruction::Branch(terminator_bb_id as _));

                self.move_forward();
                let end_bb_id = self.current_bb;
                self.bbs[terminator_bb_id]
                    .instructions
                    .push(Instruction::Branch(end_bb_id as _));
                //ret
                0
            }
            ExprKind::Ident(s) => {
                let s: &str = s;
                if self.locals.contains_key(s) {
                    let i = *self.locals.get(s).unwrap();
                    let r = i as u16;
                    let new_r = self.new_reg();
                    self.write(Instruction::Move(new_r, r));
                    return new_r;
                } else if self.env.contains_key(s) {
                    self.nenv += 1;
                    let r = self.new_reg();
                    let pos = if !self.used_upvars.contains_key(s) {
                        let pos = self.used_upvars.len();

                        self.used_upvars.insert(s.to_owned(), pos as _);
                        pos as u16
                    } else {
                        *self.used_upvars.get(s).unwrap() as u16
                    };
                    self.write(Instruction::LoadUpvalue(r, pos as _));
                    return r;
                } else {
                    let (g, n) = self.global2(&Global::Var(s.to_owned()));
                    let r = if !n {
                        let r = self.new_reg();
                        self.write(Instruction::LoadConst(r, g as _));
                        r
                    } else {
                        let (s, _) = self.global(&Global::Str(s.to_owned()));
                        let r2 = self.new_reg();
                        self.write(Instruction::LoadStaticById(r2, s as _));
                        r2
                    };
                    return r;
                }
            }
            ExprKind::Function(name, params, body) => {
                let r = self.compile_function(params, body, name.clone());
                return r;
            }
            ExprKind::New(expr) => match &expr.expr {
                ExprKind::Call(value, args) => {
                    for arg in args.iter().rev() {
                        let r = self.compile(arg, tail);
                        self.write(Instruction::Push(r));
                    }
                    let value = self.compile(value, tail);
                    let r = self.new_reg();
                    self.write(Instruction::New(r, value, args.len() as _));
                    r
                }
                _ => panic!("Call expected"),
            },

            ExprKind::While(cond, block) => {
                let expr_check_bb_id = self.current_bb + 1;
                self.write(Instruction::Branch(expr_check_bb_id as _));
                self.move_forward();

                let r = self.compile(cond, tail);
                let break_point_bb = self.current_bb + 1;
                self.move_forward();
                let body_begin_id = self.current_bb + 1;
                self.move_forward();
                let _ = self.with_lci(
                    LoopControlInfo {
                        break_point: break_point_bb as _,
                        continue_point: expr_check_bb_id as _,
                    },
                    |ctx| ctx.scoped(|ctx| ctx.compile(block, tail)),
                );
                self.write(Instruction::Branch(expr_check_bb_id as _));
                let end_bb_id = self.current_bb + 1;
                self.move_forward();
                self.bbs[break_point_bb]
                    .instructions
                    .push(Instruction::Branch(end_bb_id as _));
                self.bbs[expr_check_bb_id]
                    .instructions
                    .push(Instruction::ConditionalBranch(
                        r,
                        body_begin_id as _,
                        end_bb_id as _,
                    ));
                0
            }
            ExprKind::ArrayIndex(value, index) => {
                let value = self.compile(value, tail);
                let index = self.compile(index, tail);
                let r = self.new_reg();
                self.write(Instruction::LoadByValue(r, value, index));
                r
            }
            ExprKind::Call(value, args) => {
                for arg in args.iter().rev() {
                    let r = self.compile(arg, tail);
                    self.write(Instruction::Push(r));
                }
                match &value.expr {
                    ExprKind::Access(object, fields) => {
                        let this = self.compile(object, tail);
                        let field = self.new_reg();
                        let (s, _) = self.global(&Global::Str(fields.to_owned()));
                        self.write(Instruction::LoadById(field, this, s as _));
                        let r = self.new_reg();
                        self.write(Instruction::VirtCall(r, field, this, args.len() as _));
                        return r;
                    }
                    _ => (),
                }
                let value = self.compile(value, tail);
                let r = self.new_reg();
                //if tail {
                //  self.write(Instruction::TailCall(r, value, args.len() as _));
                //} else {
                self.write(Instruction::Call(r, value, args.len() as _));
                //}

                r
            }
            ExprKind::Lambda(arguments, body) => self.compile_function(arguments, body, None),
            ExprKind::Access(f, s) => {
                let acc = self.compile_access(&ExprKind::Access(f.clone(), s.clone()));
                self.access_get(acc)
            }
            ExprKind::This => {
                let r = self.new_reg();
                self.write(Instruction::LoadThis(r));
                r
            }
            ExprKind::ConstBool(val) => {
                let r = self.new_reg();
                if *val {
                    self.write(Instruction::LoadTrue(r));
                } else {
                    self.write(Instruction::LoadFalse(r));
                }
                r
            }
            expr => panic!("{:?}", expr),
        }
    }

    pub fn compile_function(
        &mut self,
        params: &[String],
        e: &Box<Expr>,
        vname: Option<String>,
    ) -> u16 {
        let mut ctx = Context {
            g: self.g.clone(),
            bbs: vec![BasicBlock {
                instructions: vec![],
                index: 0,
            }],
            pos: Vec::new(),
            limit: self.stack,
            stack: self.stack,
            locals: LinkedHashMap::new(),
            labels: self.labels.clone(),
            nenv: 0,
            env: self.locals.clone(),
            current_bb: 0,
            used_upvars: LinkedHashMap::new(),
            loop_control_info: vec![],
            cur_pos: (0, 0),
            cur_file: String::new(),
            regs: 33,
        };
        for p in params.iter().rev() {
            ctx.stack += 1;
            let r = ctx.new_reg();
            ctx.write(Instruction::Pop(r));
            ctx.locals.insert(p.to_owned(), r as _);
        }
        if vname.is_some() {
            self.global(&Global::Str(vname.as_ref().unwrap().to_owned()));
        }

        let gid = ctx.g.borrow().table.len();
        if vname.is_some() {
            ctx.g
                .borrow_mut()
                .globals
                .insert(Global::Var(vname.as_ref().unwrap().to_owned()), gid as i32);
        }
        ctx.g.borrow_mut().table.push(Global::Func(gid as i32, -1));
        let r = ctx.compile(e, true);
        if r != 0 {
            ctx.write(Instruction::Return(Some(r)));
        } else {
            let r = self.new_reg();
            self.write(Instruction::LoadNull(r));
            ctx.write(Instruction::Return(Some(r)));
        }
        //jlight_vm::runtime::fusion::optimizer::simplify_cfg(&mut ctx.bbs);

        ctx.finalize();
        //ctx.check_stack(s, "");

        ctx.g.borrow_mut().functions.push((
            ctx.bbs.clone(),
            ctx.pos.clone(),
            gid as i32,
            params.len() as i32,
            vname.unwrap_or(String::from("<anonymous>")),
        ));

        if ctx.nenv > 0 {
            for (var, _) in ctx.used_upvars.iter() {
                let r = self.ident(var);
                self.write(Instruction::Push(r));
            }
            let r = self.new_reg();
            self.write(Instruction::LoadConst(r, gid as _));

            self.write(Instruction::MakeEnv(r, (ctx.used_upvars.len()) as u16));
            return r;
        } else {
            let r = self.new_reg();
            self.write(Instruction::LoadConst(r, gid as _));
            return r;
        }
    }
    fn ident(&mut self, name: &str) -> u16 {
        let s: &str = name;
        if self.locals.contains_key(s) {
            let i = *self.locals.get(s).unwrap();
            let r = i as u16;
            return r;
        } else if self.env.contains_key(s) {
            self.nenv += 1;
            let r = self.new_reg();
            let pos = if !self.used_upvars.contains_key(s) {
                let pos = self.used_upvars.len();

                self.used_upvars.insert(s.to_owned(), pos as _);
                pos as u16
            } else {
                *self.used_upvars.get(s).unwrap() as u16
            };
            self.write(Instruction::LoadUpvalue(r, pos as _));
            return r;
        } else {
            let (g, n) = self.global2(&Global::Var(s.to_owned()));
            let r = if !n {
                let r = self.new_reg();
                self.write(Instruction::LoadConst(r, g as _));
                r
            } else {
                let (s, _) = self.global(&Global::Str(s.to_owned()));
                let r2 = self.new_reg();
                self.write(Instruction::LoadStaticById(r2, s as _));
                r2
            };
            return r;
        }
    }
    pub fn new() -> Self {
        let g = Globals {
            globals: LinkedHashMap::new(),
            objects: LinkedHashMap::new(),
            functions: vec![],
            table: vec![],
        };
        Self {
            g: Rc::new(RefCell::new(g)),
            used_upvars: LinkedHashMap::new(),
            locals: LinkedHashMap::new(),
            limit: 0,
            nenv: 0,
            bbs: vec![BasicBlock {
                instructions: vec![],
                index: 0,
            }],
            current_bb: 0,
            labels: HashMap::new(),
            loop_control_info: vec![],
            cur_pos: (0, 0),
            cur_file: String::new(),
            env: LinkedHashMap::new(),
            pos: vec![],
            regs: 33,
            stack: 0,
        }
    }

    pub fn finalize(&mut self) {
        if self.bbs.last().is_some() && self.bbs.last().unwrap().instructions.is_empty() {
            self.bbs.pop();
        }
        use passes::BytecodePass;
        use peephole::PeepholePass;
        use regalloc::RegisterAllocationPass;
        use ret_sink::RetSink;
        use simplify::SimplifyCFGPass;
        use waffle::bytecode::passes::*;
        let mut pass = RegisterAllocationPass::new();
        let mut bbs = Arc::new(self.bbs.clone());
        let mut simplify = SimplifyCFGPass;
        simplify.execute(&mut bbs);
        if bbs.last().is_some() && bbs.last().unwrap().instructions.is_empty() {
            bbs.pop();
        }
        log::trace!("Before RA: ");
        for (i, bb) in bbs.iter().enumerate() {
            log::trace!("{}:", i);
            for (i, ins) in bb.instructions.iter().enumerate() {
                log::trace!("  0x{:x}: {:?}", i, ins);
            }
        }
        pass.execute(&mut bbs);

        let mut ret_sink = RetSink;
        ret_sink.execute(&mut bbs);
        let mut simplify = SimplifyCFGPass;
        simplify.execute(&mut bbs);
        let mut peephole = PeepholePass;
        log::trace!("Before peephole: ");
        for (i, bb) in bbs.iter().enumerate() {
            log::trace!("{}:", i);
            for (i, ins) in bb.instructions.iter().enumerate() {
                log::trace!("  0x{:x}: {:?}", i, ins);
            }
        }
        peephole.execute(&mut bbs);
        log::trace!("After peephole:");
        for (i, bb) in bbs.iter().enumerate() {
            log::trace!("{}:", i);
            for (i, ins) in bb.instructions.iter().enumerate() {
                log::trace!("  0x{:x}: {:?}", i, ins);
            }
        }
        self.bbs = (*bbs).clone();
        //pass.execute(f: &mut Arc<Function>)
    }
}

pub fn compile(ast: Vec<Box<Expr>>) -> Context {
    let mut ctx = Context::new();
    let ast = Box::new(Expr {
        pos: crate::token::Position::new(0, 0),
        expr: ExprKind::Block(ast.clone()),
    });

    let r = ctx.compile(&ast, false);
    if r != 0 {
        ctx.write(Instruction::Return(Some(r)));
    } else {
        let r = ctx.new_reg();
        ctx.write(Instruction::LoadNull(r));
        ctx.write(Instruction::Return(Some(r)));
    }
    ctx.global(&Global::Str("main".to_owned()));
    ctx
}

pub fn module_from_ctx(context: &Context) -> Arc<Module> {
    let mut m = Arc::new(Module::new("Main"));
    m.globals = vec![Value::empty(); context.g.borrow().table.len()];
    let rt: &Runtime = &RUNTIME;
    for (blocks, _, gid, params, name) in context.g.borrow().functions.iter() {
        /*let f = Function {
            name: Arc::new(name.clone()),
            argc: *params as i32,
            code: Arc::new(blocks.clone()),
            module: m.clone(),
            native: None,
            upvalues: vec![],
            hotness: 0,
        };
        let object = Object::with_prototype(ObjectValue::Function(f), state.function_prototype);
        let object = state.gc.allocate(&RUNTIME.state, object);
        //let prototype = Object::with_prototype(ObjectValue::None, state.object_prototype);
        m.globals.get()[*gid as usize] = object;*/
        let interned = rt.state.intern(name);
        let fun = rt.state.allocate_fn(Function {
            name: Value::from(interned),
            argc: *params as i32,
            code: Arc::new(blocks.clone()),
            module: m.clone(),
            native: None,
            upvalues: vec![],
            md: Default::default(),
        });
        fun.add_attribute_without_barrier(
            &rt.state,
            Arc::new("prototype".to_owned()),
            rt.state.allocate(Cell::with_prototype(
                CellValue::None,
                rt.state.object_prototype.as_cell(),
            )),
        );
        m.globals[*gid as usize] = fun;
    }

    for (i, g) in context.g.borrow().table.iter().enumerate() {
        match g {
            Global::Str(x) => {
                let value = rt.state.intern(x);
                m.globals[i] = Value::from(value);
            }

            _ => (),
        }
    }
    let entry = rt.state.allocate_fn(Function {
        name: Value::from(rt.state.intern_string("main".to_owned())),
        argc: 0,
        code: Arc::new(context.bbs.clone()),
        module: m.clone(),
        native: None,
        upvalues: vec![],
        md: Default::default(),
    });
    m.globals.push(Value::from(entry));

    m
}

pub fn disassemble_module(module: &Arc<Module>) {
    for global in module.globals.iter() {
        if !global.is_cell() {
            continue;
        }
        if global.is_empty() {
            panic!();
        }
        match global.as_cell().get().value {
            CellValue::Function(ref func) => {
                println!("function {} (...)", func.name);
                for (i, bb) in func.code.iter().enumerate() {
                    println!("{}:", i);
                    for (i, ins) in bb.instructions.iter().enumerate() {
                        println!("  0x{:x}: {:?}", i, ins);
                    }
                }
            }
            _ => (),
        }
    }
}
