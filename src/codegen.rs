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
use crate::msg::*;
use crate::token::Position;
use basicblock::*;
use cell::*;
use hashlink::{LinkedHashMap, LinkedHashSet};
use instruction::*;
use module::*;
use runtime::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::rc::Rc;
use value::*;
use waffle::bytecode::*;
use waffle::runtime;
use waffle::util::arc::Arc;
use waffle::util::ptr::DerefPointer;
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
    pub immutable: LinkedHashSet<String>,
    pub locals: LinkedHashMap<String, i32>,
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
    pub parent: Option<DerefPointer<Context>>,
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
            liveout: vec![],
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
    pub fn access_set(&mut self, p: Position, acc: Access, r: u16) -> Result<u16, MsgWithPos> {
        match acc {
            Access::Env(n) => {
                self.write(Instruction::StoreUpvalue(r, n as _));
                return Ok(r);
            }
            Access::Stack(name, l) => {
                if self.immutable.contains(&name) {
                    return Err(MsgWithPos::new(p, Msg::LetReassigned));
                }
                //let l = self.new_reg();
                self.write(Instruction::Move(l as _, r));
                //self.locals.insert(name, l as _);
                return Ok(l as _);
            }
            Access::Global(_, _, _) => unimplemented!(),
            Access::Field(obj, f) => {
                let (gid, _) = self.global(&Global::Str(f.to_owned()));
                let obj = self.compile(&*obj, false)?;
                //self.write(Instruction::LoadConst(sr, gid as _));
                self.write(Instruction::StoreById(obj, r, gid as _));
                return Ok(r);
            }
            Access::Index(_) => unimplemented!(),
            Access::Array(value, index) => {
                let value = self.compile(&value, false)?;
                let index = self.compile(&index, false)?;
                self.write(Instruction::StoreByValue(value, index, r));
                return Ok(r);
            }
            //Access::This => self.write(Opcode::SetThis),
            //Access::Array => self.write(Opcode::SetArray),
            _ => unimplemented!(),
        }
    }
    pub fn access_get(&mut self, acc: Access) -> Result<u16, MsgWithPos> {
        let r = self.new_reg();
        match acc {
            Access::Env(i) => {
                self.write(Instruction::LoadUpvalue(r, i as _));
                return Ok(r);
            }
            Access::Stack(_, l) => {
                self.write(Instruction::Move(r, l as _));
                return Ok(r);
            }
            Access::Global(x, n, name) => {
                if !n {
                    self.write(Instruction::LoadConst(r, x as _));
                } else {
                    let (gid, _) = self.global(&Global::Str(name));
                    self.write(Instruction::LoadStaticById(r, gid as _));
                }
                return Ok(r);
            }
            Access::Field(e, f) => {
                let (gid, _) = self.global(&Global::Str(f));
                //let g = self.new_reg();
                //self.write(Instruction::LoadConst(g as _, gid as _));
                let o = self.compile(&*e, false)?;
                self.write(Instruction::LoadById(r as _, o as _, gid as _));
                return Ok(r);
            }
            Access::This => {
                self.write(Instruction::LoadThis(r));
                return Ok(r);
            }
            Access::Array(value, index) => {
                let value = self.compile(&value, false)?;
                let index = self.compile(&index, false)?;
                self.write(Instruction::LoadByValue(r, value, index));
                return Ok(r);
            }

            _ => unimplemented!(),
        }
    }

    pub fn access_env(&mut self, name: &str) -> Option<Access> {
        let mut current = self.parent;
        let mut prev = vec![DerefPointer::new(self)];
        while let Some(mut ctx) = current {
            let ctx: &mut Context = &mut *ctx;
            if ctx.locals.contains_key(name) {
                let mut last_pos = 0;
                let _l = ctx.locals.get(name).unwrap().clone();
                for prev in prev.iter_mut().rev() {
                    let pos: i32 = if !prev.used_upvars.contains_key(name) {
                        let pos = prev.used_upvars.len();
                        prev.used_upvars.insert(name.to_owned(), pos as _);
                        prev.nenv += 1;
                        pos as i32
                    } else {
                        *prev.used_upvars.get(name).unwrap()
                    };
                    last_pos = pos
                }
                return Some(Access::Env(last_pos as _));
            }
            current = ctx.parent;
            prev.push(DerefPointer::new(ctx));
        }

        None
    }
    pub fn compile_access(&mut self, e: &ExprKind) -> Access {
        match e {
            ExprKind::Ident(name) => {
                let l = self.locals.get(name);
                if l.is_some() {
                    let l = *l.unwrap();
                    return Access::Stack(name.to_owned(), l);
                } else {
                    if let Some(acc) = self.access_env(name) {
                        return acc;
                    } else {
                        let (g, n) = self.global(&Global::Var(name.to_owned()));
                        return Access::Global(g, n, name.to_owned());
                    }
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

    pub fn compile_binop(
        &mut self,
        op: &str,
        e1: &Expr,
        e2: &Expr,
        tail: bool,
    ) -> Result<u16, MsgWithPos> {
        match op {
            "==" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Equal, r3, r1, r2));
                Ok(r3)
            }
            "!=" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::NotEqual, r3, r1, r2));
                Ok(r3)
            }
            ">" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Greater, r3, r1, r2));
                Ok(r3)
            }
            ">=" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::GreaterOrEqual, r3, r1, r2));
                Ok(r3)
            }
            "<" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Less, r3, r1, r2));
                Ok(r3)
            }
            "<=" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::LessOrEqual, r3, r1, r2));
                Ok(r3)
            }
            "+" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Add, r3, r1, r2));
                Ok(r3)
            }
            "-" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Sub, r3, r1, r2));
                Ok(r3)
            }
            "/" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Div, r3, r1, r2));
                Ok(r3)
            }
            "*" => {
                let r1 = self.compile(e1, tail)?;
                let r2 = self.compile(e2, tail)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Mul, r3, r1, r2));
                Ok(r3)
            }
            "&&" => {
                let p = e1.pos;
                self.compile(
                    &Expr {
                        pos: p,
                        expr: ExprKind::If(
                            Box::new(e1.clone()),
                            /*Box::new(Expr {
                                pos: p,
                                expr: ExprKind::If(
                                    Box::new(e2.clone()),
                                    Box::new(Expr {
                                        pos: p,
                                        expr: ExprKind::ConstBool(true),
                                    }),
                                    Some(Box::new(Expr {
                                        pos: p,
                                        expr: ExprKind::ConstBool(false),
                                    })),
                                ),
                            }),*/
                            Box::new(e2.clone()),
                            Some(Box::new(Expr {
                                pos: p,
                                expr: ExprKind::ConstBool(false),
                            })),
                        ),
                    },
                    false,
                )
            }
            "||" => {
                let pos = e1.pos;
                self.compile(
                    &Expr {
                        pos,
                        expr: ExprKind::If(
                            Box::new(e1.clone()),
                            Box::new(Expr {
                                pos,
                                expr: ExprKind::ConstBool(true),
                            }),
                            Some(Box::new(e2.clone())),
                        ),
                    },
                    false,
                )
            }
            "%" => {
                let r1 = self.compile(e1, true)?;
                let r2 = self.compile(e2, true)?;
                let r3 = self.new_reg();
                self.write(Instruction::Binary(BinOp::Mod, r3, r1, r2));
                Ok(r3)
            }
            _ => unimplemented!(),
        }
    }

    pub fn compile(&mut self, e: &Expr, tail: bool) -> Result<u16, MsgWithPos> {
        match &e.expr {
            ExprKind::Throw(e) => {
                let r = self.compile(e, tail)?;
                self.write(Instruction::Throw(r));
                Ok(0)
            }
            ExprKind::Block(v) => {
                if v.is_empty() {
                    let r = self.new_reg();
                    let expr_next_bb = self.current_bb + 1;
                    self.write(Instruction::LoadNull(r));
                    self.write(Instruction::Branch(expr_next_bb as _));
                    self.move_forward();
                    return Ok(r);
                }
                let last = self.scoped::<Result<Option<u16>, MsgWithPos>, _>(|ctx| {
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Branch(expr_next_bb as _));
                    ctx.move_forward();
                    let mut last = None;
                    for x in v.iter() {
                        let r = ctx.compile(x, tail)?;
                        last = Some(r);
                    }
                    let expr_next_bb = ctx.current_bb + 1;
                    ctx.write(Instruction::Branch(expr_next_bb as _));
                    ctx.move_forward();
                    Ok(last)
                })?;
                match last {
                    Some(r) if r != 0 => Ok(r),
                    _ => {
                        let x = self.new_reg();
                        self.write(Instruction::LoadNull(x));
                        Ok(x)
                    }
                }
            }
            ExprKind::BinOp(e1, op, e2) => self.compile_binop(op, e1, e2, tail),
            ExprKind::ConstInt(x) => {
                let r = self.new_reg();
                if *x >= std::i32::MAX as i64 {
                    self.write(Instruction::LoadNumber(r, f64::to_bits(*x as f64)));
                    return Ok(r);
                }
                self.write(Instruction::LoadInt(r, *x as i32));
                Ok(r)
            }
            ExprKind::ConstFloat(x) => {
                let r = self.new_reg();
                self.write(Instruction::LoadNumber(r, x.to_bits()));
                Ok(r)
            }
            ExprKind::ConstStr(s) => {
                let (gid, _) = self.global(&Global::Str(s.to_owned()));
                let g = self.new_reg();
                self.write(Instruction::LoadConst(g, gid as _));
                Ok(g)
            }
            ExprKind::Return(e) => match e {
                Some(e) => {
                    let r = self.compile(e, true)?;
                    self.write(Instruction::Return(Some(r)));
                    self.move_forward();
                    Ok(r)
                }
                _ => {
                    self.write(Instruction::Return(None));
                    self.move_forward();
                    Ok(0)
                }
            },
            ExprKind::Let(mutable, pat, expr) => {
                if let ExprKind::Function(None, params, body) = &expr.expr {
                    if let PatternDecl::Ident(name) = &pat.decl {
                        let r = self.compile_function(params, body, Some(name.to_owned()))?;
                        return Ok(r);
                    }
                }
                let r = self.compile(expr, tail)?;
                self.compile_var_pattern(pat.pos, pat, *mutable, r)?;
                Ok(r)
            }
            ExprKind::Var(mutable, name, init) => {
                let r = match init {
                    Some(val) => {
                        let x = self.compile(&**val, tail)?;
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
                if self.immutable.contains(name) && *mutable {
                    self.immutable.remove(name);
                }
                self.locals.insert(name.to_owned(), r as _);

                Ok(r)
            }
            ExprKind::Assign(lhs, rhs) => {
                let a = self.compile_access(&lhs.expr);
                let r = self.compile(rhs, false)?;
                self.access_set(lhs.pos, a, r)
            }
            ExprKind::If(cond, if_true, if_false) => {
                let before_bb_id = self.current_bb;
                self.move_forward();
                let terminator_bb_id = self.current_bb;
                self.move_forward();
                let r = self.compile(cond, tail)?;
                let br_begin = self.current_bb;
                let else_begin;
                let ret = self.new_reg();
                if let Some(if_false) = if_false {
                    self.move_forward();
                    else_begin = self.current_bb;
                    let last = self.compile(if_false, tail)?;
                    self.write(Instruction::Move(ret, last));

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
                let last = self.compile(if_true, tail)?;
                self.write(Instruction::Move(ret, last));
                self.write(Instruction::Branch(terminator_bb_id as _));
                self.move_forward();
                let end_bb_id = self.current_bb;
                self.bbs[terminator_bb_id]
                    .instructions
                    .push(Instruction::Branch(end_bb_id as _));
                Ok(ret)
            }
            ExprKind::Ident(s) => {
                /*let s: &str = s;
                if self.locals.contains_key(s) {
                    let i = *self.locals.get(s).unwrap();
                    let r = i as u16;
                    let new_r = self.new_reg();
                    self.write(Instruction::Move(new_r, r));
                    return Ok(new_r);
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
                    return Ok(r);
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
                    return Ok(r);
                }*/
                Ok(self.ident(s))
            }
            ExprKind::Function(name, params, body) => {
                let r = self.compile_function(params, body, name.clone())?;
                return Ok(r);
            }
            ExprKind::New(expr) => match &expr.expr {
                ExprKind::Call(value, args) => {
                    for arg in args.iter().rev() {
                        let r = self.compile(arg, tail)?;
                        self.write(Instruction::Push(r));
                    }
                    let value = self.compile(value, tail)?;
                    let r = self.new_reg();
                    self.write(Instruction::New(r, value, args.len() as _));
                    Ok(r)
                }
                _ => panic!("Call expected"),
            },

            ExprKind::While(cond, block) => {
                let r = self.scoped::<Result<u16, MsgWithPos>, _>(|fb| {
                    let expr_check_bb_id = fb.current_bb as u16 + 1;
                    fb.write(Instruction::Branch(expr_check_bb_id));
                    fb.move_forward();
                    let r = fb.compile(cond, tail)?;
                    let break_bb_id = fb.current_bb as u16 + 1;
                    fb.move_forward();
                    let body_bb_id = fb.current_bb as u16 + 1;
                    fb.move_forward();

                    let last = fb.with_lci(
                        LoopControlInfo {
                            break_point: break_bb_id,
                            continue_point: expr_check_bb_id,
                        },
                        |fb| fb.compile(block, tail),
                    )?;
                    fb.write(Instruction::GcSafepoint);
                    fb.write(Instruction::Branch(expr_check_bb_id));
                    let end_bb_id = fb.current_bb as u16 + 1;
                    fb.move_forward();
                    let end2_bb_id = fb.current_bb as u16 + 1;
                    fb.move_forward();
                    let next = fb.current_bb as u16 + 1;
                    fb.move_forward();
                    fb.bbs[break_bb_id as usize]
                        .instructions
                        .push(Instruction::Branch(end2_bb_id));
                    fb.bbs[expr_check_bb_id as usize]
                        .instructions
                        .push(Instruction::ConditionalBranch(r, body_bb_id, end_bb_id));
                    fb.bbs[end_bb_id as usize]
                        .instructions
                        .push(Instruction::Branch(next));
                    fb.bbs[end2_bb_id as usize]
                        .instructions
                        .push(Instruction::Branch(next));
                    fb.write(Instruction::GcSafepoint);
                    Ok(last)
                })?;
                Ok(r)
            }
            ExprKind::ArrayIndex(value, index) => {
                let value = self.compile(value, tail)?;
                let index = self.compile(index, tail)?;
                let r = self.new_reg();
                self.write(Instruction::LoadByValue(r, value, index));
                Ok(r)
            }
            ExprKind::Call(value, args) => {
                for arg in args.iter().rev() {
                    let r = self.compile(arg, tail)?;
                    self.write(Instruction::Push(r));
                }
                match &value.expr {
                    ExprKind::Access(object, fields) => {
                        let this = self.compile(object, tail)?;
                        let field = self.new_reg();
                        let (s, _) = self.global(&Global::Str(fields.to_owned()));
                        self.write(Instruction::LoadById(field, this, s as _));
                        let r = self.new_reg();
                        self.write(Instruction::VirtCall(r, field, this, args.len() as _));
                        return Ok(r);
                    }
                    _ => (),
                }
                let value = self.compile(value, tail)?;
                let r = self.new_reg();
                self.write(Instruction::Call(r, value, args.len() as _));
                Ok(r)
            }
            ExprKind::Unop(op, val) => {
                let r = self.compile(val, tail)?;
                let dest = self.new_reg();
                let op: &str = op;
                match op {
                    "-" => self.write(Instruction::Unary(UnaryOp::Neg, dest, r)),
                    "!" => self.write(Instruction::Unary(UnaryOp::Not, dest, r)),
                    _ => self.write(Instruction::Move(dest, r)),
                }
                Ok(dest)
            }
            ExprKind::Lambda(arguments, body) => self.compile_function(arguments, body, None),
            ExprKind::Access(f, s) => {
                let acc = self.compile_access(&ExprKind::Access(f.clone(), s.clone()));
                self.access_get(acc)
            }
            ExprKind::This => {
                let r = self.new_reg();
                self.write(Instruction::LoadThis(r));
                Ok(r)
            }
            ExprKind::ConstBool(val) => {
                let r = self.new_reg();
                if *val {
                    self.write(Instruction::LoadTrue(r));
                } else {
                    self.write(Instruction::LoadFalse(r));
                }
                Ok(r)
            }
            ExprKind::Match(e2, patterns) => self.compile_match(e.pos, e2, patterns),
            ExprKind::Nil => {
                let r = self.new_reg();
                self.write(Instruction::LoadNull(r));
                Ok(r)
            }
            expr => panic!("{:?}", expr),
        }
    }
    pub fn compile_match(
        &mut self,
        _: Position,
        e: &Box<Expr>,
        patterns: &[(Box<Pattern>, Option<Box<Expr>>, Box<Expr>)],
    ) -> Result<u16, MsgWithPos> {
        let value = self.compile(e, false)?;
        let mut to_terminate = vec![];

        for (pattern, when, body) in patterns.iter() {
            let tmp = self.locals.clone();
            let r = self.compile_pattern(pattern.pos, pattern, value)?;
            let first = self.current_bb;
            let first_r = r;
            //self.write(Instruction::BranchIfFalse(r, 0));
            self.move_forward();
            let first_next = self.current_bb;
            let second = self.current_bb;
            let mut second_r = None;
            let mut second_b = None;
            if let Some(when) = when {
                let r = self.compile(when, false)?;
                second_r = Some(r);
                //self.write(Instruction::BranchIfFalse(r, 0));
                self.move_forward();
                second_b = Some(self.current_bb);
            }
            let r = self.compile(body, false)?;
            self.write(Instruction::Move(0, r));
            to_terminate.push(self.current_bb);
            self.move_forward();
            let terminator_id = self.current_bb;
            self.bbs[first]
                .instructions
                .push(Instruction::ConditionalBranch(
                    first_r,
                    first_next as _,
                    terminator_id as _,
                ));
            if let Some(second_r) = second_r {
                self.bbs[second]
                    .instructions
                    .push(Instruction::ConditionalBranch(
                        second_r,
                        second_b.unwrap() as u16,
                        terminator_id as _,
                    ));
            }
            self.locals = tmp;
        }
        //self.move_forward();
        let id = self.current_bb;
        for bb in to_terminate.iter() {
            self.bbs[*bb]
                .instructions
                .push(Instruction::Branch(id as _));
        }
        let r = self.new_reg();
        self.write(Instruction::Move(r, 0));
        Ok(0)
    }

    fn compile_pattern(
        &mut self,
        _p: Position,
        pat: &Pattern,
        val: u16,
    ) -> Result<u16, MsgWithPos> {
        match &pat.decl {
            PatternDecl::ConstChar(c) => {
                let r = self.new_reg();
                let (gid, _) = self.global(&Global::Str(c.to_string()));
                let r2 = self.new_reg();
                self.write(Instruction::LoadConst(r, gid as _));
                self.write(Instruction::Binary(BinOp::Equal, r2, r, val));
                Ok(r2)
            }
            PatternDecl::ConstFloat(f) => {
                let r = self.new_reg();
                let r2 = self.new_reg();
                self.write(Instruction::LoadNumber(r, f.to_bits()));
                self.write(Instruction::Binary(BinOp::Equal, r2, r, val));
                Ok(r2)
            }
            PatternDecl::ConstInt(f) => {
                let r = self.new_reg();
                let r2 = self.new_reg();
                if *f < std::i32::MAX as i64 {
                    self.write(Instruction::LoadInt(r, *f as i32));
                } else {
                    self.write(Instruction::LoadNumber(r, (*f as f64).to_bits()))
                }
                self.write(Instruction::Binary(BinOp::Equal, r2, r, val));
                Ok(r2)
            }
            PatternDecl::ConstStr(s) => {
                let r = self.new_reg();
                let r2 = self.new_reg();
                let (gid, _) = self.global(&Global::Str(s.to_owned()));
                self.write(Instruction::LoadConst(r, gid as _));
                self.write(Instruction::Binary(BinOp::Equal, r2, r, val));
                Ok(r2)
            }
            PatternDecl::Ident(name) => {
                self.immutable.insert(name.to_owned());
                let r = self.new_reg();
                self.write(Instruction::Move(r, val));
                self.locals.insert(name.to_owned(), r as _);
                let r2 = self.new_reg();
                self.write(Instruction::LoadTrue(r2));
                Ok(r2)
            }
            PatternDecl::Record(fields) => {
                let mut branches = vec![];
                for (name, pat) in fields.iter() {
                    let r = self.new_reg();
                    let (gid, _) = self.global(&Global::Str(name.to_owned()));
                    self.write(Instruction::LoadById(r, val, gid as _));
                    if let Some(pat) = pat {
                        let r = self.compile_pattern(pat.pos, pat, r)?;
                        self.write(Instruction::Move(0, r));
                        branches.push((self.current_bb, self.current_bb + 1));
                    } else {
                        self.write(Instruction::LoadTrue(0));
                    }
                    self.move_forward();
                    //self.immutable.insert(name.to_owned());
                    //self.locals.insert(name.to_owned(), r as _);
                }
                let terminator_bb_id = self.current_bb;
                for (branch, next) in branches {
                    self.bbs[branch]
                        .instructions
                        .push(Instruction::ConditionalBranch(
                            0,
                            next as _,
                            terminator_bb_id as _,
                        ));
                }
                let r = self.new_reg();
                self.write(Instruction::Move(r, 0));
                Ok(r)
            }
            PatternDecl::Array(patterns) => {
                let mut branches = vec![];
                for (i, pat) in patterns.iter().enumerate() {
                    let r = self.new_reg();
                    let r2 = self.new_reg();
                    self.write(Instruction::LoadInt(r2, i as i32));
                    self.write(Instruction::LoadByValue(r, val, r2));
                    let r = self.compile_pattern(pat.pos, pat, r)?;
                    self.write(Instruction::Move(0, r));
                    branches.push((self.current_bb, self.current_bb + 1));
                    self.move_forward();
                }
                let terminator_bb_id = self.current_bb;
                for (branch, next) in branches {
                    self.bbs[branch]
                        .instructions
                        .push(Instruction::ConditionalBranch(
                            0,
                            next as _,
                            terminator_bb_id as _,
                        ));
                }
                let r = self.new_reg();
                self.write(Instruction::Move(r, 0));
                Ok(r)
            }
            PatternDecl::Pass => {
                self.write(Instruction::Branch(self.current_bb as u16 + 1));
                self.move_forward();
                let r = self.new_reg();
                self.write(Instruction::LoadTrue(r));
                Ok(r)
            }
            _ => unimplemented!(),
        }
    }
    pub fn compile_var_pattern(
        &mut self,
        pos: Position,
        pat: &Box<Pattern>,
        mutable: bool,
        r: u16,
    ) -> Result<(), MsgWithPos> {
        match &pat.decl {
            PatternDecl::Array(patterns) => {
                for (i, pat) in patterns.iter().enumerate() {
                    let nr = self.new_reg();
                    self.write(Instruction::LoadInt(nr, i as i32));
                    let val = self.new_reg();
                    self.write(Instruction::LoadByValue(val, r, nr));
                    self.compile_var_pattern(pat.pos, pat, mutable, val)?;
                }
            }
            PatternDecl::Ident(name) => {
                if !mutable {
                    self.immutable.insert(name.clone());
                } else if self.immutable.contains(name) && mutable {
                    self.immutable.remove(name);
                }
                let loc = self.new_reg();
                self.write(Instruction::Move(loc, r));
                self.locals.insert(name.to_owned(), loc as _);
                return Ok(());
            }
            PatternDecl::Record(fields) => {
                for (name, p) in fields.iter() {
                    if let Some(pat) = p {
                        return Err(MsgWithPos::new(
                            pat.pos,
                            Msg::Custom("unexpected pattern in variable declaration".to_owned()),
                        ));
                    }
                    if !mutable {
                        self.immutable.insert(name.clone());
                    }
                    let loc = self.new_reg();
                    self.write(Instruction::Move(loc, r));
                    self.locals.insert(name.to_owned(), loc as _);
                }
            }
            PatternDecl::Pass => (),
            _ => {
                return Err(MsgWithPos::new(
                    pos,
                    Msg::Custom(
                        "record,array or ident pattern expected in variable declaration".to_owned(),
                    ),
                ))
            }
        }
        Ok(())
    }

    pub fn compile_arg(&mut self, p: Position, arg: &Arg) -> Result<(), MsgWithPos> {
        match arg {
            Arg::Ident(mutable, name) => {
                if self.locals.contains_key(name) {
                    return Err(MsgWithPos::new(
                        p,
                        Msg::Custom(format!("argument '{}' already defined", name)),
                    ));
                }
                if !*mutable {
                    self.immutable.insert(name.clone());
                }

                let r = self.new_reg();
                self.write(Instruction::Pop(r));
                self.locals.insert(name.to_owned(), r as _);
                Ok(())
            }
            Arg::Record(rec) => {
                let obj = self.new_reg();
                self.write(Instruction::Pop(obj));
                for item in rec {
                    if self.locals.contains_key(item) {
                        return Err(MsgWithPos::new(
                            p,
                            Msg::Custom(format!("argument '{}' already defined", item)),
                        ));
                    }

                    let field = self.new_reg();
                    let (id, _) = self.global(&Global::Str(item.to_owned()));
                    self.write(Instruction::LoadById(field, obj, id as _));
                    self.locals.insert(item.to_owned(), field as _);
                }
                Ok(())
            }
            Arg::Array(arr) => {
                let obj = self.new_reg();
                self.write(Instruction::Pop(obj));
                for (i, item) in arr.iter().enumerate() {
                    if self.locals.contains_key(item) {
                        return Err(MsgWithPos::new(
                            p,
                            Msg::Custom(format!("argument '{}' already defined", item)),
                        ));
                    }
                    let field = self.new_reg();
                    let id = self.new_reg();
                    self.write(Instruction::LoadInt(id, i as i32));
                    self.write(Instruction::LoadByValue(field, obj, id));
                    self.locals.insert(item.to_owned(), field as _);
                }
                Ok(())
            }
        }
    }

    pub fn compile_function(
        &mut self,
        params: &[Arg],
        e: &Box<Expr>,
        vname: Option<String>,
    ) -> Result<u16, MsgWithPos> {
        let mut ctx = Context {
            immutable: LinkedHashSet::new(),
            g: self.g.clone(),
            bbs: vec![BasicBlock {
                instructions: vec![],
                liveout: vec![],
                index: 0,
            }],
            pos: Vec::new(),
            limit: self.stack,
            stack: self.stack,
            locals: LinkedHashMap::new(),
            labels: self.labels.clone(),
            nenv: 0,
            current_bb: 0,
            used_upvars: LinkedHashMap::new(),
            loop_control_info: vec![],
            cur_pos: (0, 0),
            cur_file: String::new(),
            regs: 33,
            parent: Some(DerefPointer::new(self)),
        };
        for p in params.iter().rev() {
            ctx.compile_arg(e.pos, p)?;
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
        let r = ctx.compile(e, true)?;
        ctx.write(Instruction::Branch(ctx.current_bb as u16 + 1));
        ctx.move_forward();
        if r != 0 {
            ctx.write(Instruction::Return(Some(r)));
        } else {
            let r = ctx.new_reg();
            ctx.write(Instruction::LoadNull(r));
            ctx.write(Instruction::Return(Some(r)));
        }
        //jlight_vm::runtime::fusion::optimizer::simplify_cfg(&mut ctx.bbs);

        ctx.finalize(
            true,
            vname
                .clone()
                .map(|x| x.to_owned())
                .unwrap_or("<>".to_owned()),
        );
        //ctx.check_stack(s, "");

        ctx.g.borrow_mut().functions.push((
            ctx.bbs.clone(),
            ctx.pos.clone(),
            gid as i32,
            params.len() as i32,
            vname.unwrap_or(String::from("<anonymous>")),
        ));

        if ctx.nenv > 0 {
            for (var, _) in ctx.used_upvars.iter().rev() {
                let r = self.ident(var);
                self.write(Instruction::Push(r));
            }
            let r = self.new_reg();
            self.write(Instruction::LoadConst(r, gid as _));

            self.write(Instruction::MakeEnv(r, (ctx.used_upvars.len()) as u16));
            return Ok(r);
        } else {
            let r = self.new_reg();
            self.write(Instruction::LoadConst(r, gid as _));
            return Ok(r);
        }
    }
    fn ident(&mut self, name: &str) -> u16 {
        let s: &str = name;
        if self.locals.contains_key(s) {
            let i = *self.locals.get(s).unwrap();
            let r = i as u16;
            return r;
        } else {
            if let Some(Access::Env(pos)) = self.access_env(name) {
                let r = self.new_reg();
                self.write(Instruction::LoadUpvalue(r, pos as _));
                return r;
            }
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
            immutable: LinkedHashSet::new(),
            limit: 0,
            nenv: 0,
            bbs: vec![BasicBlock {
                instructions: vec![],
                liveout: vec![],
                index: 0,
            }],
            current_bb: 0,
            labels: HashMap::new(),
            loop_control_info: vec![],
            cur_pos: (0, 0),
            cur_file: String::new(),
            pos: vec![],
            regs: 33,
            stack: 0,
            parent: None,
        }
    }

    pub fn finalize(&mut self, _tail: bool, _name: String) {}
}

pub fn compile(ast: Vec<Box<Expr>>, no_std: bool) -> Result<Context, MsgWithPos> {
    let mut ctx = Context::new();
    let ast = Box::new(Expr {
        pos: crate::token::Position::new(0, 0),
        expr: ExprKind::Block(ast.clone()),
    });
    if !no_std {
        let (r1, r2) = (ctx.new_reg(), ctx.new_reg());
        let (gid, _) = ctx.global(&Global::Str("__start__".to_owned()));
        ctx.write(Instruction::LoadStaticById(r1, gid as _));
        ctx.write(Instruction::Call(r2, r1, 0));
    }
    ctx.global(&Global::Str("<anonymous>".to_owned()));
    let r = ctx.compile(&ast, false)?;
    ctx.write(Instruction::Branch(ctx.current_bb as u16 + 1));
    ctx.move_forward();
    if r != 0 {
        ctx.write(Instruction::Return(Some(r)));
    } else {
        let r = ctx.new_reg();
        ctx.write(Instruction::LoadNull(r));
        ctx.write(Instruction::Return(Some(r)));
    }
    ctx.global(&Global::Str("main".to_owned()));
    Ok(ctx)
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

        m.globals[*gid as usize] = Value::from(fun.as_cell());
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
    m.globals.push(Value::from(entry.as_cell()));

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
