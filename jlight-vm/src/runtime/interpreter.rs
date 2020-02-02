use super::fusion::tracing_interpreter::*;
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
        $context = $process.context_ptr();
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
    pub fn run(&self, thread: &mut Arc<JThread>) -> ObjectPointer {
        let mut context: crate::util::ptr::Ptr<Context>;
        let mut index;
        let mut bindex;
        let mut instruction;
        reset_context!(thread, context, bindex, index);
        'exec_loop: loop {
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
                        if let Some(value) = value {
                            let x = context.get_register(value);
                            return x;
                        } else {
                            return self.allocate_null();
                        }
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
                        return self.allocate_null();
                    }
                    reset_context!(thread, context, index, bindex);
                    safepoint(&self.state);
                }
                Instruction::TailCall(return_register, function, argc) => {
                    let function = context.get_register(function);
                    if function.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function.get().value {
                            ObjectValue::Function(ref function) => {
                                if let None = function.native {
                                    let mut new_stack = vec![];
                                    for _ in 0..argc {
                                        new_stack.push(
                                            context.stack.pop().unwrap_or(self.state.nil_prototype),
                                        )
                                    }
                                    thread.pop_context();
                                    let mut new_ctx = Context::new();
                                    new_ctx.stack = new_stack;
                                    new_ctx.code = function.code.clone();
                                    new_ctx.bp = 0;
                                    new_ctx.module = function.module.clone();
                                    new_ctx.upvalues =
                                        function.upvalues.iter().map(|x| x.clone()).collect();
                                    thread.push_context(new_ctx);
                                    reset_context!(thread, context, index, bindex);
                                } else if let Some(native) = function.native {
                                    let mut args = vec![];
                                    for _ in 0..argc {
                                        args.push(
                                            context.stack.pop().unwrap_or(self.state.nil_prototype),
                                        )
                                    }
                                    context.set_register(
                                        return_register,
                                        native(self, ObjectPointer::null(), &args).unwrap(),
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
                    let function_object = context.get_register(function);
                    if function_object.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function_object.get_mut().value {
                            ObjectValue::Function(ref mut function) => {
                                if let None = function.native {
                                    if function.hotness >= 10 {
                                        let mut info =
                                            super::fusion::tracing_interpreter::TRACE_INFO
                                                .with(|x| x.clone());
                                        if info
                                            .entry(function_object)
                                            .or_insert(TraceInfo {
                                                invocations: 0,
                                                trace: vec![
                                                    super::fusion::instruction::FusionBasicBlock {
                                                        instructions: vec![]
                                                    };
                                                    context.code.len()
                                                ],
                                                current_block: 0,
                                                complete: std::collections::HashSet::new(),
                                            })
                                            .complete
                                            .len()
                                            == function.code.len()
                                        {
                                            new_ctx.code = function.code.clone();
                                            new_ctx.function = function_object;
                                            new_ctx.bp = 0;
                                            function.hotness += 1;
                                            new_ctx.module = function.module.clone();
                                            new_ctx.upvalues = function
                                                .upvalues
                                                .iter()
                                                .map(|x| x.clone())
                                                .collect();
                                            thread.push_context(new_ctx);

                                            enter_context!(thread, context, index, bindex);
                                            continue;
                                        }
                                        let (result, _) = self
                                            .run_function_with_thread_and_tracing(
                                                function_object,
                                                thread,
                                                &mut *info,
                                                &new_ctx.stack,
                                            );
                                        drop(info);
                                        context.set_register(return_register, result);
                                    } else {
                                        new_ctx.code = function.code.clone();
                                        new_ctx.function = function_object;
                                        new_ctx.bp = 0;
                                        function.hotness += 1;
                                        new_ctx.module = function.module.clone();
                                        new_ctx.upvalues =
                                            function.upvalues.iter().map(|x| x.clone()).collect();
                                        thread.push_context(new_ctx);

                                        enter_context!(thread, context, index, bindex);
                                    }
                                } else if let Some(native) = function.native {
                                    context.set_register(
                                        return_register,
                                        native(self, ObjectPointer::null(), &new_ctx.stack)
                                            .unwrap(),
                                    );
                                }
                            }
                            _ => unimplemented!(), // TODO try/catch support
                        }
                    }
                    safepoint(&self.state);
                }
                Instruction::Push(r) => {
                    let r = context.get_register(r);
                    context.stack.push(r);
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
                    if let ObjectValue::Function(ref function) = prototype.get().value {
                        if let None = function.native {
                            let mut new_ctx = Context::new();
                            new_ctx.return_register = Some(return_register);
                            for _ in 0..argc {
                                new_ctx
                                    .stack
                                    .push(context.stack.pop().unwrap_or(self.state.nil_prototype));
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
                                args.push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                            }
                            context
                                .set_register(return_register, native(self, this, &args).unwrap());
                        }
                    } else {
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
                                            new_ctx.upvalues = function
                                                .upvalues
                                                .iter()
                                                .map(|x| x.clone())
                                                .collect();
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
                                                native(self, this, &args).unwrap(),
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
                    let function_object = context.get_register(function);
                    if function_object.is_tagged_number() {
                        unimplemented!() // TODO: try/catch support
                    } else {
                        match function_object.get().value {
                            ObjectValue::Function(ref function) => {
                                if let None = function.native {
                                    new_ctx.code = function.code.clone();
                                    new_ctx.bp = 0;
                                    new_ctx.module = function.module.clone();
                                    new_ctx.this = this;
                                    new_ctx.function = function_object;
                                    new_ctx.upvalues =
                                        function.upvalues.iter().map(|x| x.clone()).collect();
                                    thread.push_context(new_ctx);
                                    enter_context!(thread, context, index, bindex);
                                } else if let Some(native) = function.native {
                                    context.set_register(
                                        return_register,
                                        native(self, this, &new_ctx.stack).unwrap(),
                                    );
                                }
                            }
                            _ => panic!("{:?}", function), // TODO try/catch support
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
                    match &mut object.get_mut().value {
                        ObjectValue::Array(ref mut array) => {
                            if key.is_tagged_number() {
                                let idx = key.number_value().unwrap() as usize;
                                if idx >= array.len() {
                                    for _ in array.len()..=idx {
                                        array.push(self.state.nil_prototype)
                                    }
                                }
                                context
                                    .set_register(to, array[key.number_value().unwrap() as usize]);
                                continue;
                            }
                        }
                        _ => (),
                    }
                    let key = catch!(thread, context, key.as_string());

                    let value = object.lookup_attribute(&self.state, key);
                    context.set_register(to, value.unwrap_or(self.state.nil_prototype));
                }
                Instruction::Store(object, key, value) => {
                    let object = context.get_register(object);
                    let key = context.get_register(key);
                    match &mut object.get_mut().value {
                        ObjectValue::Array(array) => {
                            if key.is_tagged_number() {
                                let idx = key.number_value().unwrap() as usize;
                                if idx >= array.len() {
                                    for _ in array.len()..=idx {
                                        array.push(self.state.nil_prototype);
                                    }
                                }

                                array[idx] = context.get_register(value);
                                continue;
                            }
                        }
                        _ => (),
                    }
                    let value = context.get_register(value);
                    let key = catch!(thread, context, key.as_string());

                    object.add_attribute(&key, value);
                }

                Instruction::LoadStatic(to, key) => {
                    let key = context.module.globals.get()[key as usize]
                        .as_string()
                        .unwrap();
                    let value = self
                        .state
                        .static_variables
                        .get(&**key)
                        .expect(&format!("Static '{}' variable not found", key));
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
                        bindex = y as usize;
                        index = 0;
                    } else {
                        bindex = x as usize;
                        index = 0;
                    }
                }

                Instruction::Goto(block) => {
                    bindex = block as usize;
                    index = 0;
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
                Instruction::Add(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            ObjectPointer::number(
                                r1.number_value().unwrap() + r2.number_value().unwrap(),
                            ),
                        );
                        continue 'exec_loop;
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(r1.number_value().unwrap() + y),
                            ),
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(y + r2.number_value().unwrap()),
                            ),
                            ObjectValue::String(ref string) => {
                                let s = self.allocate_string(Arc::new(format!(
                                    "{}{}",
                                    string,
                                    r2.to_string()
                                )));
                                context.set_register(r0, s);
                            }
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                }
                Instruction::Sub(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            ObjectPointer::number(
                                r1.number_value().unwrap() - r2.number_value().unwrap(),
                            ),
                        );
                        continue 'exec_loop;
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(r1.number_value().unwrap() - y),
                            ),
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(y - r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                }
                Instruction::Mul(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            ObjectPointer::number(
                                r1.number_value().unwrap() * r2.number_value().unwrap(),
                            ),
                        );
                        continue 'exec_loop;
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(r1.number_value().unwrap() * y),
                            ),
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(y * r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                }
                Instruction::Div(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            ObjectPointer::number(
                                r1.number_value().unwrap() / r2.number_value().unwrap(),
                            ),
                        );
                        continue 'exec_loop;
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(r1.number_value().unwrap() / y),
                            ),
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                ObjectPointer::number(y / r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                }
                Instruction::Equal(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() == r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() == y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y == r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::Function(_), ObjectValue::Function(_)) => r1 == r2,
                            (ObjectValue::Module(_), ObjectValue::Module(_)) => r1 == r2,
                            (ObjectValue::String(x), ObjectValue::String(y)) => x == y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x == y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x == y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x == y,
                            (ObjectValue::Array(_), ObjectValue::Array(_)) => r1 == r2,
                            _ => r1 == r2,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }
                Instruction::NotEqual(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() != r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() != y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y != r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::Function(_), ObjectValue::Function(_)) => r1 != r2,
                            (ObjectValue::Module(_), ObjectValue::Module(_)) => r1 == r2,
                            (ObjectValue::String(x), ObjectValue::String(y)) => x != y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x != y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x != y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x != y,
                            (ObjectValue::Array(_), ObjectValue::Array(_)) => r1 != r2,
                            _ => r1 == r2,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }
                Instruction::Greater(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() > r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() > y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y > r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::String(x), ObjectValue::String(y)) => x > y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x > y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x > y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x > y,
                            (ObjectValue::Array(x), ObjectValue::Array(y)) => x.len() > y.len(),
                            _ => false,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }

                Instruction::GreaterEqual(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() >= r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() >= y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y >= r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::String(x), ObjectValue::String(y)) => x >= y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x >= y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x >= y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x >= y,
                            (ObjectValue::Array(x), ObjectValue::Array(y)) => x.len() >= y.len(),
                            _ => false,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }

                Instruction::LessEqual(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() <= r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() <= y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y <= r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::String(x), ObjectValue::String(y)) => x <= y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x <= y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x <= y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x <= y,
                            (ObjectValue::Array(x), ObjectValue::Array(y)) => x.len() <= y.len(),
                            _ => false,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }
                Instruction::Less(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() < r2.number_value().unwrap(),
                            ),
                        );
                    } else if r1.is_tagged_number() {
                        match r2.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(r1.number_value().unwrap() < y),
                            ),
                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else if r2.is_tagged_number() {
                        match r1.get().value {
                            ObjectValue::Number(y) => context.set_register(
                                r0,
                                self.allocate_bool(y < r2.number_value().unwrap()),
                            ),

                            _ => context.set_register(r0, self.allocate_bool(false)),
                        }
                    } else {
                        let value = match (&r1.get().value, &r2.get().value) {
                            (ObjectValue::String(x), ObjectValue::String(y)) => x < y,
                            (ObjectValue::ByteArray(x), ObjectValue::ByteArray(y)) => x < y,
                            (ObjectValue::Number(x), ObjectValue::Number(y)) => x < y,
                            (ObjectValue::Bool(x), ObjectValue::Bool(y)) => x < y,
                            (ObjectValue::Array(x), ObjectValue::Array(y)) => x.len() < y.len(),
                            _ => false,
                        };
                        context.set_register(r0, self.allocate_bool(value));
                    }
                }
                Instruction::Not(r0, r1) => {
                    let r1 = context.get_register(r1);
                    if r1.is_tagged_number() {
                        context.set_register(
                            r0,
                            ObjectPointer::number(
                                (!(r1.number_value().unwrap().floor() as i64)) as f64,
                            ),
                        );
                    } else {
                        match r1.get().value {
                            ObjectValue::Bool(x) => {
                                context.set_register(r0, self.allocate_bool(!x))
                            }
                            ObjectValue::Number(x) => context.set_register(
                                r0,
                                ObjectPointer::number((!(x.floor() as i64)) as f64),
                            ),
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                }
                Instruction::BoolAnd(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    let x = !r1.is_false(&self.state);
                    let y = !r2.is_false(&self.state);
                    context.set_register(r0, self.allocate_bool(x && y));
                }
                Instruction::BoolOr(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    let x = !r1.is_false(&self.state);
                    let y = !r2.is_false(&self.state);
                    context.set_register(r0, self.allocate_bool(x || y));
                }
                Instruction::LoadNull(x) => context.set_register(x, self.allocate_null()),

                x => panic!("{:?}", x),
            }
        }
    }

    pub fn allocate_null(&self) -> ObjectPointer {
        self.state.nil_prototype
    }

    pub fn allocate_string(&self, s: Arc<String>) -> ObjectPointer {
        let object = Object::with_prototype(ObjectValue::String(s), self.state.string_prototype);
        self.state.gc.allocate(object)
    }

    pub fn allocate_bool(&self, x: bool) -> ObjectPointer {
        let object = Object::with_prototype(ObjectValue::Bool(x), self.state.boolean_prototype);
        self.state.gc.allocate(object)
    }
}
