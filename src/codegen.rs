use crate::ast::*;
use block::*;
use framework::*;
use instructions::*;
use jlight_vm::bytecode::*;
use jlight_vm::runtime::module::*;
use jlight_vm::runtime::object::*;
use jlight_vm::runtime::value::*;
use jlight_vm::runtime::*;
use jlight_vm::util::shared::Arc;
use std::cell::RefCell;
use std::collections::HashMap;
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
use hashlink::*;

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
    pub regs: u32,
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
    pub fn access_set(&mut self, acc: Access, r: u32) {
        match acc {
            Access::Env(n) => {
                self.write(Instruction::StoreU(r, n as _));
            }
            Access::Stack(name, _) => {
                let l = self.new_reg();
                self.write(Instruction::Move(l as _, r));
                self.locals.insert(name, l as _);
            }
            Access::Global(_, _, _) => unimplemented!(),
            Access::Field(obj, f) => {
                let (gid, _) = self.global(&Global::Str(f.to_owned()));
                let sr = self.new_reg();
                let obj = self.compile(&*obj, false);
                self.write(Instruction::LoadGlobal(sr, gid as _));
                self.write(Instruction::Store(obj, sr, r));
            }
            Access::Index(_) => unimplemented!(),
            Access::Array(value, index) => {
                let value = self.compile(&value, false);
                let index = self.compile(&index, false);
                self.write(Instruction::Store(value, index, r));
            }
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
            Access::Stack(_, l) => {
                self.write(Instruction::Move(r, l as _));
                return r;
            }
            Access::Global(x, n, name) => {
                if !n {
                    self.write(Instruction::LoadGlobal(r, x as _));
                } else {
                    let (gid, _) = self.global(&Global::Str(name));
                    self.write(Instruction::LoadStatic(r, gid as _));
                }
                return r;
            }
            Access::Field(e, f) => {
                let (gid, _) = self.global(&Global::Str(f));
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
            Access::Array(value, index) => {
                let value = self.compile(&value, false);
                let index = self.compile(&index, false);
                self.write(Instruction::Load(r, value, index));
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

    pub fn load_args(&mut self, _names: &[String]) {}

    pub fn compile_binop(&mut self, op: &str, e1: &Expr, e2: &Expr, tail: bool) -> u32 {
        match op {
            "==" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Equal(r3, r1, r2));
                r3
            }
            "!=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::NotEqual(r3, r1, r2));
                r3
            }
            ">" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Greater(r3, r1, r2));
                r3
            }
            ">=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::GreaterEqual(r3, r1, r2));
                r3
            }
            "<" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Less(r3, r1, r2));
                r3
            }
            "<=" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::LessEqual(r3, r1, r2));
                r3
            }
            "+" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Add(r3, r1, r2));
                r3
            }
            "-" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Sub(r3, r1, r2));
                r3
            }
            "/" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Div(r3, r1, r2));
                r3
            }
            "*" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::Mul(r3, r1, r2));
                r3
            }
            "&&" => {
                let r1 = self.compile(e1, tail);
                let r2 = self.compile(e2, tail);
                let r3 = self.new_reg();
                self.write(Instruction::BoolAnd(r3, r1, r2));
                r3
            }
            _ => unimplemented!(),
        }
    }

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
                if v.is_empty() {
                    let r = self.new_reg();
                    self.write(Instruction::LoadNull(r));
                    return r;
                }
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
            ExprKind::BinOp(e1, op, e2) => self.compile_binop(op, e1, e2, tail),
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
                let (gid, _) = self.global(&Global::Str(s.to_owned()));
                let g = self.new_reg();
                self.write(Instruction::LoadGlobal(g, gid as _));
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
                self.access_set(a, r);
                r
            }
            ExprKind::If(cond, if_true, if_false) => {
                let before_bb_id = self.current_bb;
                self.move_forward();
                let terminator_bb_id = self.current_bb;
                self.move_forward();
                let r = self.compile(cond, tail);
                let br_begin = self.current_bb;
                let else_begin;
                let ret = self.new_reg();
                if let Some(if_false) = if_false {
                    self.move_forward();
                    else_begin = self.current_bb;
                    let last = self.compile(if_false, tail);
                    self.write(Instruction::Move(ret, last));
                    self.get_current_bb()
                        .instructions
                        .push(Instruction::Goto(terminator_bb_id as _));
                } else {
                    self.write(Instruction::LoadNull(ret));
                    else_begin = terminator_bb_id;
                }
                self.move_forward();
                self.bbs[before_bb_id]
                    .instructions
                    .push(Instruction::Goto(br_begin as _));
                let checker_bb = br_begin;
                let current_bb = self.current_bb;
                self.bbs[checker_bb]
                    .instructions
                    .push(Instruction::ConditionalGoto(
                        r,
                        current_bb as _,
                        else_begin as _,
                    ));
                self.compile(if_true, tail);
                self.write(Instruction::Goto(terminator_bb_id as _));

                self.move_forward();
                let end_bb_id = self.current_bb;
                self.bbs[terminator_bb_id]
                    .instructions
                    .push(Instruction::Goto(end_bb_id as _));
                ret
            }
            ExprKind::Ident(s) => {
                let s: &str = s;
                if self.locals.contains_key(s) {
                    let i = *self.locals.get(s).unwrap();
                    let r = i as u32;
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
                    self.write(Instruction::LoadU(r, pos as _));
                    return r;
                } else {
                    let (g, n) = self.global2(&Global::Var(s.to_owned()));
                    let r = if !n {
                        let r = self.new_reg();
                        self.write(Instruction::LoadGlobal(r, g as _));
                        r
                    } else {
                        let (s, _) = self.global(&Global::Str(s.to_owned()));
                        let r2 = self.new_reg();
                        self.write(Instruction::LoadStatic(r2, s as _));
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
                    self.write(Instruction::Construct(r, value, args.len() as _));
                    r
                }
                _ => panic!("Call expected"),
            },
            ExprKind::While(cond, block) => {
                let expr_check_bb_id = self.current_bb + 1;
                self.write(Instruction::Goto(expr_check_bb_id as _));
                self.move_forward();

                let r = self.compile(cond, tail);
                let break_point_bb = self.current_bb + 1;
                self.move_forward();
                let body_begin_id = self.current_bb + 1;
                self.move_forward();
                let ret = self.with_lci(
                    LoopControlInfo {
                        break_point: break_point_bb as _,
                        continue_point: expr_check_bb_id as _,
                    },
                    |ctx| ctx.compile(block, tail),
                );
                self.write(Instruction::Goto(expr_check_bb_id as _));
                let end_bb_id = self.current_bb + 1;
                self.move_forward();
                self.bbs[break_point_bb]
                    .instructions
                    .push(Instruction::Goto(end_bb_id as _));
                self.bbs[expr_check_bb_id]
                    .instructions
                    .push(Instruction::ConditionalGoto(
                        r,
                        body_begin_id as _,
                        end_bb_id as _,
                    ));
                ret
            }
            ExprKind::ArrayIndex(value, index) => {
                let value = self.compile(value, tail);
                let index = self.compile(index, tail);
                let r = self.new_reg();
                self.write(Instruction::Load(r, value, index));
                r
            }
            ExprKind::Call(value, args) => {
                for arg in args.iter() {
                    let r = self.compile(arg, tail);
                    self.write(Instruction::Push(r));
                }
                match &value.expr {
                    ExprKind::Access(object, fields) => {
                        let this = self.compile(object, tail);
                        let field = self.new_reg();
                        let (s, _) = self.global(&Global::Str(fields.to_owned()));
                        let sr = self.new_reg();
                        self.write(Instruction::LoadGlobal(sr, s as _));
                        self.write(Instruction::Load(field, this, sr));
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
            expr => panic!("{:?}", expr),
        }
    }

    fn ident(&mut self, name: &str) -> u32 {
        let s: &str = name;
        if self.locals.contains_key(s) {
            let i = *self.locals.get(s).unwrap();
            let r = i as u32;
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
            self.write(Instruction::LoadU(r, pos as _));
            return r;
        } else {
            let (g, n) = self.global2(&Global::Var(s.to_owned()));
            let r = if !n {
                let r = self.new_reg();
                self.write(Instruction::LoadGlobal(r, g as _));
                r
            } else {
                let (s, _) = self.global(&Global::Str(s.to_owned()));
                let r2 = self.new_reg();
                self.write(Instruction::LoadStatic(r2, s as _));
                r2
            };
            return r;
        }
    }

    pub fn compile_function(
        &mut self,
        params: &[String],
        e: &Box<Expr>,
        vname: Option<String>,
    ) -> u32 {
        let mut ctx = Context {
            g: self.g.clone(),
            bbs: vec![BasicBlock {
                instructions: vec![],
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
            regs: 0,
        };
        for p in params.iter().rev() {
            ctx.stack += 1;
            let r = ctx.new_reg();
            ctx.write(Instruction::Pop(r));
            ctx.locals.insert(p.to_owned(), r as _);
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
        ctx.write(Instruction::Return(Some(r)));
        jlight_vm::runtime::fusion::optimizer::simplify_cfg(&mut ctx.bbs);

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
            self.write(Instruction::LoadGlobal(r, gid as _));

            self.write(Instruction::MakeEnv(r, (ctx.used_upvars.len()) as u32));
            return r;
        } else {
            let r = self.new_reg();
            self.write(Instruction::LoadGlobal(r, gid as _));
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
            }],
            current_bb: 0,
            labels: HashMap::new(),
            loop_control_info: vec![],
            cur_pos: (0, 0),
            cur_file: String::new(),
            env: LinkedHashMap::new(),
            pos: vec![],
            regs: 0,
            stack: 0,
        }
    }

    pub fn finalize(&mut self) {
        jlight_vm::runtime::fusion::optimizer::simplify_cfg(&mut self.bbs);
        if self.bbs.last().is_some() && self.bbs.last().unwrap().instructions.is_empty() {
            self.bbs.pop();
        }
        let mut bfc = BytecodeFunction::new();
        for (i, bb) in self.bbs.iter().enumerate() {
            bfc.block(i, regalloc::TypedIxVec::from_vec(bb.instructions.clone()));
        }

        regalloc(&mut bfc);
        self.bbs = bfc.to_basic_blocks();
        jlight_vm::runtime::fusion::optimizer::remove_bad_moves(&mut self.bbs);
    }
}

pub fn compile(ast: Vec<Box<Expr>>) -> Context {
    let mut ctx = Context::new();
    let ast = Box::new(Expr {
        pos: crate::token::Position::new(0, 0),
        expr: ExprKind::Block(ast.clone()),
    });

    let r = ctx.compile(&ast, false);
    ctx.write(Instruction::Return(Some(r)));
    ctx
}

use jlight_vm::runtime::state::RcState;
use jlight_vm::util::ptr::Ptr;
pub fn module_from_ctx(context: &Context, state: &RcState) -> Arc<Module> {
    let mut m = Arc::new(Module::new());
    m.globals = Ptr::new(vec![
        Value::from(VTag::Undefined);
        context.g.borrow().table.len()
    ]);

    for (blocks, _, gid, params, name) in context.g.borrow().functions.iter() {
        let f = Function {
            name: Arc::new(name.clone()),
            argc: *params as i32,
            code: Ptr::new(blocks.clone()),
            module: m.clone(),
            native: None,
            upvalues: vec![],
            hotness: 0,
        };
        let object = Object::with_prototype(ObjectValue::Function(f), state.function_prototype);
        let object = state.gc.allocate(&RUNTIME.state, object);
        //let prototype = Object::with_prototype(ObjectValue::None, state.object_prototype);
        m.globals.get()[*gid as usize] = object;
    }

    for (i, g) in context.g.borrow().table.iter().enumerate() {
        match g {
            Global::Str(x) => {
                let value = ObjectValue::String(Arc::new(x.to_owned()));
                m.globals.get()[i] = state.gc.allocate(
                    &RUNTIME.state,
                    Object::with_prototype(value, Value::from(VTag::Null)),
                );
            }

            _ => (),
        }
    }
    let entry = Function {
        name: Arc::new(String::from("main")),
        argc: 0,
        code: Ptr::new(context.bbs.clone()),
        module: m.clone(),
        native: None,
        upvalues: vec![],
        hotness: 0,
    };
    let object = Object::with_prototype(ObjectValue::Function(entry), state.function_prototype);
    m.globals
        .get()
        .push(state.gc.allocate(&RUNTIME.state, object));

    m
}

pub fn disassemble_module(module: &Arc<Module>) {
    for global in module.globals.get().iter() {
        if !global.is_cell() {
            continue;
        }
        match global.as_cell().get().value {
            ObjectValue::Function(ref func) => {
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
