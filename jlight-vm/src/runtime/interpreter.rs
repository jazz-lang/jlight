use super::string_pool::{intern, str};
use super::threads::*;
use super::*;
use crate::bytecode::{block::BasicBlock, instructions::Instruction};
use crate::heap::global::safepoint;
use crate::util::arc::Arc;
use context::*;
use object::*;
macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident,$bindex: ident) => {{
        $context = $process.context_mut();
        $index = $context.ip;
        $bindex = $context.bp;
    }};
}

macro_rules! enter_context {
    ($process:expr, $context:ident, $index:ident,$bindex: ident) => {{
        $context.ip = $index;
        $context.bp = $bindex;
        reset_context!($process, $context, $index, $bindex);
    }};
}

macro_rules! catch {
    ($process: expr, $context: expr,$value: expr) => {{
        $value.unwrap()
    }};
}

impl Runtime {
    pub fn run(&self, thread: &Arc<JThread>) {
        let mut context: &mut Context;
        let mut index;
        let mut bindex;
        let mut instruction;
        reset_context!(thread, context, bindex, index);
        'exec_loop: loop {
            if index >= unsafe { context.code.get_unchecked(bindex).instructions.len() } {
                bindex += 1;
                index = 0;
            }
            let block: &BasicBlock = unsafe { context.code.get_unchecked(bindex) };
            instruction = block.instructions[index].clone();
            index += 1;
            match instruction {
                Instruction::LoadInt(r, val) => {
                    context.set_register(r, ObjectPointer::number(val as i64 as f64))
                }
                Instruction::LoadNum(r, val) => {
                    context.set_register(r, ObjectPointer::number(f64::from_bits(val)));
                }

                Instruction::Move(to, from) => {
                    context.move_(to, from);
                }
                Instruction::Safepoint => safepoint(&self.state),
                Instruction::Return(value) => {
                    if context.terminate_upon_return {
                        break 'exec_loop;
                    }

                    let object = if let Some(value) = value {
                        context.get_register(value)
                    } else {
                        self.state.nil_prototype
                    };

                    if let Some(register) = context.return_register {
                        if let Some(context) = context.parent.as_mut() {
                            context.set_register(register, object);
                        }
                    }

                    if thread.pop_context() {
                        break 'exec_loop;
                    }
                    reset_context!(thread, context, index, bindex);
                    safepoint(&self.state);
                }
                Instruction::TailCall(return_register, function, argc) => {
                    let mut new_ctx = Context::new();
                    new_ctx.return_register = Some(return_register);
                    for _ in 0..argc {
                        new_ctx
                            .stack
                            .push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                    }
                    let function = context.get_register(function);
                    if function.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function.get().value {
                            ObjectValue::Function(ref function) => {
                                if let None = function.native {
                                    thread.pop_context();
                                    new_ctx.code = function.code.clone();
                                    new_ctx.bp = 0;
                                    new_ctx.module = function.module.clone();
                                    new_ctx.upvalues =
                                        function.upvalues.iter().map(|x| x.clone()).collect();
                                    thread.push_context(new_ctx);

                                    reset_context!(thread, context, index, bindex);
                                } else if let Some(native) = function.native {
                                    context.set_register(
                                        return_register,
                                        native(self, ObjectPointer::null(), &new_ctx.stack),
                                    );
                                }
                            }
                            _ => unimplemented!(), // TODO try/catch support
                        }
                    }
                    safepoint(&self.state);
                }
                Instruction::Call(return_register, function, argc) => {
                    let mut new_ctx = Context::new();
                    new_ctx.return_register = Some(return_register);
                    for _ in 0..argc {
                        new_ctx
                            .stack
                            .push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                    }
                    let function = context.get_register(function);
                    if function.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function.get().value {
                            ObjectValue::Function(ref function) => {
                                if let None = function.native {
                                    new_ctx.code = function.code.clone();
                                    new_ctx.bp = 0;
                                    new_ctx.module = function.module.clone();
                                    new_ctx.upvalues =
                                        function.upvalues.iter().map(|x| x.clone()).collect();
                                    thread.push_context(new_ctx);

                                    enter_context!(thread, context, index, bindex);
                                } else if let Some(native) = function.native {
                                    context.set_register(
                                        return_register,
                                        native(self, ObjectPointer::null(), &new_ctx.stack),
                                    );
                                }
                            }
                            _ => unimplemented!(), // TODO try/catch support
                        }
                    }
                    safepoint(&self.state);
                }
                Instruction::Push(r) => {
                    context.stack.push(context.get_register(r));
                }
                Instruction::Pop(r) => {
                    let v = context.stack.pop().unwrap_or(self.state.nil_prototype);
                    context.set_register(r, v);
                }
                Instruction::Construct(return_register, prototype, argc) => {
                    let prototype = context.get_register(prototype);
                    let this = self
                        .state
                        .gc
                        .allocate(Object::with_prototype(ObjectValue::None, prototype));
                    let initializer =
                        prototype.lookup_attribute(&self.state, &str(intern("init")).0);
                    if let Some(initializer) = initializer {
                        if initializer.is_tagged_number() {
                            context.set_register(return_register, this);
                        } else {
                            match initializer.get().value {
                                ObjectValue::Function(ref function) => {
                                    if let None = function.native {
                                        let mut new_ctx = Context::new();
                                        new_ctx.return_register = Some(return_register);
                                        for _ in 0..argc {
                                            new_ctx.stack.push(
                                                context
                                                    .stack
                                                    .pop()
                                                    .unwrap_or(self.state.nil_prototype),
                                            );
                                        }
                                        new_ctx.this = this;
                                        new_ctx.code = function.code.clone();
                                        new_ctx.bp = 0;
                                        new_ctx.module = function.module.clone();
                                        new_ctx.this = this;
                                        new_ctx.upvalues =
                                            function.upvalues.iter().map(|x| x.clone()).collect();
                                        thread.push_context(new_ctx);
                                        enter_context!(thread, context, index, bindex);
                                    } else if let Some(native) = function.native {
                                        let mut args = vec![];
                                        for _ in 0..argc {
                                            args.push(
                                                context
                                                    .stack
                                                    .pop()
                                                    .unwrap_or(self.state.nil_prototype),
                                            );
                                        }
                                        context.set_register(
                                            return_register,
                                            native(self, this, &args),
                                        );
                                    }
                                }
                                _ => context.set_register(return_register, this),
                            }
                        }
                    } else {
                        context.set_register(return_register, this);
                    }
                }
                Instruction::VirtCall(return_register, function, this, argc) => {
                    let mut new_ctx = Context::new();
                    new_ctx.return_register = Some(return_register);
                    for _ in 0..argc {
                        new_ctx
                            .stack
                            .push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                    }
                    let this = context.get_register(this);
                    let function = context.get_register(function);
                    if function.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function.get().value {
                            ObjectValue::Function(ref function) => {
                                if let None = function.native {
                                    new_ctx.code = function.code.clone();
                                    new_ctx.bp = 0;
                                    new_ctx.module = function.module.clone();
                                    new_ctx.this = this;
                                    new_ctx.upvalues =
                                        function.upvalues.iter().map(|x| x.clone()).collect();
                                    thread.push_context(new_ctx);
                                    enter_context!(thread, context, index, bindex);
                                } else if let Some(native) = function.native {
                                    context.set_register(
                                        return_register,
                                        native(self, this, &new_ctx.stack),
                                    );
                                }
                            }
                            _ => unimplemented!(), // TODO try/catch support
                        }
                    }
                    safepoint(&self.state);
                }
                Instruction::LoadThis(r0) => {
                    let this = context.this;
                    context.set_register(r0, this);
                }
                Instruction::Load(to, object, key) => {
                    let object = context.get_register(object);
                    let key = context.get_register(key);
                    let key = catch!(thread, context, key.as_string());

                    let value = object.lookup_attribute(&self.state, key);
                    context.set_register(to, value.unwrap_or(self.state.nil_prototype));
                }
                Instruction::Store(object, key, value) => {
                    let object = context.get_register(object);
                    let key = context.get_register(key);
                    let value = context.get_register(value);
                    let key = catch!(thread, context, key.as_string());

                    object.add_attribute(&key, value);
                }

                Instruction::LoadStatic(to, key) => {
                    let value = self
                        .state
                        .static_variables
                        .get(
                            &**context.module.globals.get()[key as usize]
                                .as_string()
                                .unwrap(),
                        )
                        .expect("Static variable not found");
                    context.set_register(to, *value);
                }
                Instruction::LoadGlobal(to, var) => {
                    let value = context.module.globals.get()[var as usize];
                    context.set_register(to, value);
                }
                Instruction::LoadU(r0, r1) => {
                    let upvar = *context
                        .upvalues
                        .get(r1 as usize)
                        .expect("upvalue not found");
                    context.set_register(r0, upvar);
                }
                Instruction::StoreU(r0, r1) => {
                    let value = context.get_register(r0);
                    context.upvalues[r1 as usize] = value;
                }
                Instruction::LoadStack(r0, ss0) => {
                    let value = context.stack[ss0 as usize];
                    context.set_register(r0, value);
                }
                Instruction::StoreStack(r0, ss0) => {
                    let value = context.get_register(r0);
                    context.stack[ss0 as usize] = value;
                }
                Instruction::ConditionalGoto(r0, x, y) => {
                    let value = context.get_register(r0);
                    if value.is_false(&self.state) {
                        bindex = y as _;
                    } else {
                        bindex = x as _;
                    }
                }

                Instruction::Goto(block) => {
                    bindex = block as usize;
                }
                Instruction::GotoIfFalse(r0, block) => {
                    let value = context.get_register(r0);
                    if value.is_false(&self.state) {
                        bindex = block as usize;
                    }
                }
                Instruction::GotoIfTrue(r0, block) => {
                    let value = context.get_register(r0);
                    if !value.is_false(&self.state) {
                        bindex = block as usize;
                    }
                }
                Instruction::MakeEnv(function, size) => {
                    let mut values = Vec::with_capacity(size as _);
                    for _ in 0..size {
                        values.push(context.stack.pop().unwrap());
                    }
                    let function = context.get_register(function);
                    assert!(!function.is_tagged_number());
                    match function.get_mut().value {
                        ObjectValue::Function(ref mut function) => {
                            function.upvalues = values;
                        }
                        _ => unreachable!(),
                    }
                }

                _ => unimplemented!(),
            }
        }
    }
}
