use super::instruction::*;
use crate::bytecode::block::*;
use crate::bytecode::instructions::*;
use crate::heap::global::*;
use crate::runtime;
use crate::util::arc::Arc;
use context::*;
use object::*;
use parking_lot::RwLock;
use regalloc::{Reg, RegClass};
use runtime::*;
use std::collections::HashMap;
use string_pool::*;
use threads::*;

thread_local! {
    pub static TRACE_INFO: Arc<HashMap<ObjectPointer,TraceInfo>> = Arc::new(HashMap::new());
}

macro_rules! vreg {
    ($class: expr,$x: expr) => {
        Reg::new_virtual($class, $x)
    };
    ($x: expr) => {
        Reg::new_virtual(RegClass::I64, $x)
    };
}
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
use std::collections::HashSet;
pub struct TraceInfo {
    pub invocations: usize,
    pub trace: Vec<FusionBasicBlock>,
    pub complete: HashSet<usize>,
    pub current_block: usize,
}

impl TraceInfo {
    pub fn write(&mut self, x: FusionInstruction) {
        if !self.complete.contains(&self.current_block) {
            self.trace[self.current_block].instructions.push(x);
        }
    }
    pub fn complete(&self) -> bool {
        let x = self.trace.iter().all(|x| !x.instructions.is_empty());
        x
    }

    pub fn move_forward(&mut self) {
        self.current_block += 1;
    }

    pub fn goto(&mut self, x: usize) {
        self.complete.insert(self.current_block);
        self.current_block = x;
    }
}

pub enum Guard {
    Greater(u32, u32),
    Less(u32, u32),
    GreaterEqual(u32, u32),
    LessEqual(u32, u32),
    Equal(u32, u32),
    NotEqual(u32, u32),
    Number(u32),
    Array(u32),
    Int8(u32),
    Int16(u32),
    Int32(u32),
    Int64(u32),
}

pub enum TraceEvent {
    Instruction(FusionInstruction),
    Guard(Guard),
}

impl Runtime {
    pub fn run_tracing(
        &self,
        thread: &Arc<JThread>,
        trace_info: &mut std::collections::HashMap<ObjectPointer, TraceInfo>,
    ) -> (ObjectPointer, bool) {
        let mut context: &mut Context;
        let mut index;
        let mut bindex;
        let mut instruction;
        reset_context!(thread, context, bindex, index);
        let trace: &mut TraceInfo = trace_info.entry(context.function).or_insert(TraceInfo {
            invocations: 0,
            trace: vec![
                FusionBasicBlock {
                    instructions: vec![]
                };
                context.code.len()
            ],
            current_block: 0,
            complete: HashSet::new(),
        });
        println!("trace loop");
        'exec_loop: loop {
            let block: &BasicBlock = unsafe { context.code.get_unchecked(bindex) };
            instruction = block.instructions[index].clone();
            //println!("{:?}", instruction);
            index += 1;
            match instruction {
                Instruction::LoadInt(r, val) => {
                    trace.write(FusionInstruction::LoadNumber(
                        vreg!(r),
                        (val as i64 as f64).to_bits(),
                    ));
                    context.set_register(r, ObjectPointer::number(val as i64 as f64))
                }
                Instruction::LoadNum(r, val) => {
                    trace.write(FusionInstruction::LoadNumber(vreg!(r), val));
                    context.set_register(r, ObjectPointer::number(f64::from_bits(val)));
                }

                Instruction::Move(to, from) => {
                    trace.write(FusionInstruction::Move(vreg!(to), vreg!(from)));
                    context.move_(to, from);
                }
                Instruction::Safepoint => {
                    trace.write(FusionInstruction::Safepoint);
                    safepoint(&self.state);
                }
                Instruction::Return(value) => {
                    if context.terminate_upon_return {
                        safepoint(&self.state);
                        trace.write(FusionInstruction::Return(value.map(|x| vreg!(x))));
                        eprintln!("Tracing finished!\nPrinting Fusion instuctions: ");
                        for (i, x) in trace.trace.iter().enumerate() {
                            eprintln!("{}:", i);
                            for ins in x.instructions.iter() {
                                eprintln!("  {:?}", ins);
                            }
                        }
                        if let Some(value) = value {
                            let x = context.get_register(value);
                            println!("return {}", x.to_string());
                            return (x, trace.complete.len() == context.code.len());
                        } else {
                            return (
                                self.allocate_null(),
                                trace.complete.len() == context.code.len(),
                            );
                        }
                    }
                    unreachable!(); /*
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
                                    safepoint(&self.state);*/
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
                    trace.write(FusionInstruction::Call(
                        vreg!(return_register),
                        vreg!(function),
                        argc as _,
                    ));
                    let mut new_ctx = Context::new();
                    new_ctx.terminate_upon_return = true;
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
                                if function.argc != -1
                                    && function.argc as usize != new_ctx.stack.len()
                                {
                                    panic!(
                                        "Expected arguments {} found {}",
                                        function.argc,
                                        new_ctx.stack.len()
                                    )
                                }
                                if let None = function.native {
                                    if function.hotness >= 10 {
                                        let mut info = TRACE_INFO.with(|x| x.clone());
                                        if info
                                            .entry(function_object)
                                            .or_insert(TraceInfo {
                                                invocations: 0,
                                                trace: vec![
                                                    FusionBasicBlock {
                                                        instructions: vec![]
                                                    };
                                                    context.code.len()
                                                ],
                                                current_block: 0,
                                                complete: std::collections::HashSet::new(),
                                            })
                                            .complete()
                                        {
                                            function.hotness += 1;
                                            let mut thread = JThread::new();
                                            new_ctx.code = function.code.clone();
                                            new_ctx.upvalues =
                                                function.upvalues.iter().map(|x| *x).collect();
                                            new_ctx.bp = 0;
                                            new_ctx.module = function.module.clone();
                                            new_ctx.return_register = Some(return_register);
                                            new_ctx.terminate_upon_return = true;
                                            new_ctx.function = function_object;
                                            thread.push_context(new_ctx);
                                            //println!("Done?");
                                            let result = self.run_function_with_thread(
                                                function_object,
                                                &mut thread,
                                            );
                                            context.set_register(return_register, result);
                                            continue;
                                        }
                                        let mut thread = JThread::new();
                                        println!("run");
                                        let (result, _) = self
                                            .run_function_with_thread_and_tracing(
                                                function_object,
                                                &mut thread,
                                                &mut *info,
                                                &new_ctx.stack,
                                            );
                                        println!("Done!");
                                        context.set_register(return_register, result);
                                        continue;
                                    } else {
                                        new_ctx.code = function.code.clone();
                                        new_ctx.function = function_object;
                                        new_ctx.bp = 0;
                                        function.hotness += 1;
                                        new_ctx.module = function.module.clone();
                                        new_ctx.upvalues =
                                            function.upvalues.iter().map(|x| x.clone()).collect();
                                        new_ctx.terminate_upon_return = true;
                                        let mut thread = JThread::new();
                                        thread.push_context(new_ctx);
                                        println!("Invoke!");
                                        let result = self
                                            .run_function_with_thread(function_object, &mut thread);
                                        context.set_register(return_register, result);
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
                    trace.write(FusionInstruction::Push(vreg!(r)));
                    context.stack.push(context.get_register(r));
                }
                Instruction::Pop(r) => {
                    trace.write(FusionInstruction::Pop(vreg!(r)));
                    let v = context.stack.pop().unwrap_or(self.state.nil_prototype);
                    context.set_register(r, v);
                }
                Instruction::Construct(return_register, prototype, argc) => {
                    trace.write(FusionInstruction::Construct(
                        vreg!(return_register),
                        vreg!(prototype),
                        argc as _,
                    ));
                    let prototype = context.get_register(prototype);
                    let this = self
                        .state
                        .gc
                        .allocate(Object::with_prototype(ObjectValue::None, prototype));
                    context.set_register(return_register, this);
                    if let ObjectValue::Function(ref function) = prototype.get().value {
                        if let None = function.native {
                            let mut new_ctx = Context::new();
                            new_ctx.return_register = None;
                            for _ in 0..argc {
                                new_ctx
                                    .stack
                                    .push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                            }
                            if function.argc != -1 && function.argc as usize != new_ctx.stack.len()
                            {
                                panic!(
                                    "Expected arguments {} found {}",
                                    function.argc,
                                    new_ctx.stack.len()
                                )
                            }
                            new_ctx.this = this;
                            new_ctx.code = function.code.clone();
                            new_ctx.bp = 0;
                            new_ctx.module = function.module.clone();
                            new_ctx.this = this;
                            new_ctx.upvalues =
                                function.upvalues.iter().map(|x| x.clone()).collect();
                            let mut thread = JThread::new();
                            new_ctx.function = prototype;
                            thread.push_context(new_ctx);
                            let _ = RUNTIME.run_function_with_thread(prototype, &mut thread);

                        //enter_context!(thread, context, index, bindex);
                        } else if let Some(native) = function.native {
                            let mut args = vec![];
                            for _ in 0..argc {
                                args.push(context.stack.pop().unwrap_or(self.state.nil_prototype));
                            }
                            if function.argc != -1 && function.argc as usize != args.len() {
                                panic!("Expected arguments {} found {}", function.argc, args.len())
                            }
                            native(self, this, &args).unwrap();
                            /*context
                            .set_register(return_register,);*/
                        }
                    } else {
                        let initializer =
                            prototype.lookup_attribute(&self.state, &str(intern("init")).0);
                        if let Some(initializer) = initializer {
                            if initializer.is_tagged_number() {
                            } else {
                                match initializer.get().value {
                                    ObjectValue::Function(ref function) => {
                                        if let None = function.native {
                                            let mut new_ctx = Context::new();
                                            new_ctx.return_register = None;
                                            for _ in 0..argc {
                                                new_ctx.stack.push(
                                                    context
                                                        .stack
                                                        .pop()
                                                        .unwrap_or(self.state.nil_prototype),
                                                );
                                            }
                                            if function.argc != -1
                                                && function.argc as usize != new_ctx.stack.len()
                                            {
                                                panic!(
                                                    "Expected arguments {} found {}",
                                                    function.argc,
                                                    new_ctx.stack.len()
                                                )
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
                                            let mut thread = JThread::new();
                                            thread.push_context(new_ctx);
                                            let _ = RUNTIME
                                                .run_function_with_thread(initializer, &mut thread);
                                        } else if let Some(native) = function.native {
                                            let mut args = vec![];
                                            if function.argc != -1
                                                && function.argc as usize != argc as usize
                                            {
                                                panic!(
                                                    "Expected arguments {} found {}",
                                                    function.argc, argc
                                                )
                                            }
                                            for _ in 0..argc {
                                                args.push(
                                                    context
                                                        .stack
                                                        .pop()
                                                        .unwrap_or(self.state.nil_prototype),
                                                );
                                            }
                                            native(self, this, &args).unwrap();
                                            /*context.set_register(
                                                return_register,
                                                ,
                                            );*/
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
                    trace.write(FusionInstruction::VirtCall(
                        vreg!(return_register),
                        vreg!(function),
                        vreg!(this),
                        argc as _,
                    ));
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
                                if function.argc != -1
                                    && function.argc as usize != new_ctx.stack.len()
                                {
                                    panic!(
                                        "Expected arguments {} found {}",
                                        function.argc,
                                        new_ctx.stack.len()
                                    )
                                }
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
                    trace.write(FusionInstruction::LoadThis(vreg!(r0)));
                    let this = context.this;
                    context.set_register(r0, this);
                }
                Instruction::Load(to, object, key) => {
                    trace.write(FusionInstruction::StoreField(
                        vreg!(to),
                        vreg!(object),
                        vreg!(key),
                    ));
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
                    trace.write(FusionInstruction::StoreField(
                        vreg!(object),
                        vreg!(key),
                        vreg!(value),
                    ));
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
                    trace.write(FusionInstruction::LoadStatic(vreg!(to), *value));
                    context.set_register(to, *value);
                }
                Instruction::LoadGlobal(to, var) => {
                    let value = context.module.globals.get()[var as usize];
                    trace.write(FusionInstruction::LoadStatic(vreg!(to), value));
                    context.set_register(to, value);
                }
                Instruction::LoadU(r0, r1) => {
                    trace.write(FusionInstruction::LoadU(vreg!(r0), r1));
                    let upvar = *context
                        .upvalues
                        .get(r1 as usize)
                        .expect("upvalue not found");
                    context.set_register(r0, upvar);
                }
                Instruction::StoreU(r0, r1) => {
                    trace.write(FusionInstruction::StoreU(vreg!(r0), r1));
                    let value = context.get_register(r0);
                    context.upvalues[r1 as usize] = value;
                }
                Instruction::LoadStack(r0, ss0) => {
                    trace.write(FusionInstruction::LoadStack(vreg!(r0), ss0 as _));
                    let value = context.stack[ss0 as usize];
                    context.set_register(r0, value);
                }
                Instruction::StoreStack(r0, ss0) => {
                    trace.write(FusionInstruction::StoreStack(vreg!(r0), ss0 as _));
                    let value = context.get_register(r0);
                    context.stack[ss0 as usize] = value;
                }
                Instruction::ConditionalGoto(r0, x, y) => {
                    trace.write(FusionInstruction::ConditionalGoto(vreg!(r0), x, y));
                    let value = context.get_register(r0);
                    if value.is_false() {
                        bindex = y as usize;
                        index = 0;
                        trace.goto(y as _);
                    } else {
                        bindex = x as usize;
                        trace.goto(x as _);
                        index = 0;
                    }
                }

                Instruction::Goto(block) => {
                    trace.write(FusionInstruction::Goto(block));
                    trace.goto(block as _);
                    bindex = block as usize;
                    index = 0;
                }
                Instruction::GotoIfFalse(r0, block) => {
                    trace.write(FusionInstruction::GotoIfFalse(vreg!(r0), block));
                    let value = context.get_register(r0);
                    if value.is_false() {
                        bindex = block as usize;
                        trace.goto(block as _);
                    }
                }
                Instruction::GotoIfTrue(r0, block) => {
                    trace.write(FusionInstruction::GotoIfTrue(vreg!(r0), block));
                    let value = context.get_register(r0);
                    if !value.is_false() {
                        bindex = block as usize;
                        trace.goto(block as _);
                    }
                }
                Instruction::MakeEnv(function, size) => {
                    trace.write(FusionInstruction::GuardFunction(vreg!(function)));
                    trace.write(FusionInstruction::MakeEnv(vreg!(function), size));
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
                Instruction::Add(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::AddF(vreg!(r0), vreg!(r1_), vreg!(r2_)));
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
                                trace.write(FusionInstruction::GuardString(vreg!(r1_)));
                                trace.write(FusionInstruction::Concat(
                                    vreg!(r0),
                                    vreg!(r1_),
                                    vreg!(r2_),
                                ));
                                let s = self.allocate_string(Arc::new(format!(
                                    "{}{}",
                                    string,
                                    r2.to_string()
                                )));
                                context.set_register(r0, s);
                                continue;
                            }
                            _ => context.set_register(r0, ObjectPointer::number(std::f64::NAN)),
                        }
                    }
                    println!("wut");
                    trace.write(FusionInstruction::Add(vreg!(r0), vreg!(r1_), vreg!(r2_)));
                }
                Instruction::Sub(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::MulF(vreg!(r0), vreg!(r1_), vreg!(r2_)));
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
                    trace.write(FusionInstruction::Sub(vreg!(r0), vreg!(r1_), vreg!(r2_)));
                }
                Instruction::Mul(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::MulF(vreg!(r0), vreg!(r1_), vreg!(r2_)));
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
                    trace.write(FusionInstruction::Mul(vreg!(r0), vreg!(r1_), vreg!(r2_)));
                }
                Instruction::Div(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::DivF(vreg!(r0), vreg!(r1_), vreg!(r2_)));
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
                    trace.write(FusionInstruction::Div(vreg!(r0), vreg!(r1_), vreg!(r2_)));
                }
                Instruction::Equal(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        /*let n1 = r1.number_value().unwrap();
                        let n2 = r2.number_value().unwrap();*/
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::NotEqual,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() == r2.number_value().unwrap(),
                            ),
                        );
                        continue;
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
                    trace.write(FusionInstruction::CmpF(
                        Cmp::Equal,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
                }
                Instruction::NotEqual(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        let n1 = r1.number_value().unwrap();
                        let n2 = r2.number_value().unwrap();

                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::NotEqual,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(r0, self.allocate_bool(n1 != n2));
                        continue;
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
                    trace.write(FusionInstruction::Cmp(
                        Cmp::NotEqual,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
                }
                Instruction::Greater(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::Greater,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() > r2.number_value().unwrap(),
                            ),
                        );
                        continue;
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
                    trace.write(FusionInstruction::Cmp(
                        Cmp::Greater,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
                }

                Instruction::GreaterEqual(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::GreaterEqual,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() >= r2.number_value().unwrap(),
                            ),
                        );
                        continue;
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
                    trace.write(FusionInstruction::Cmp(
                        Cmp::GreaterEqual,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
                }

                Instruction::LessEqual(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::LessEqual,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() <= r2.number_value().unwrap(),
                            ),
                        );
                        continue;
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
                    trace.write(FusionInstruction::Cmp(
                        Cmp::LessEqual,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
                }
                Instruction::Less(r0, r1_, r2_) => {
                    let r1 = context.get_register(r1_);
                    let r2 = context.get_register(r2_);
                    if r1.is_tagged_number() && r2.is_tagged_number() {
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r1_)));
                        trace.write(FusionInstruction::GuardNumber(vreg!(RegClass::I64, r2_)));
                        trace.write(FusionInstruction::CmpF(
                            Cmp::Less,
                            vreg!(RegClass::I64, r0),
                            vreg!(RegClass::I64, r1_),
                            vreg!(RegClass::I64, r2_),
                        ));
                        context.set_register(
                            r0,
                            self.allocate_bool(
                                r1.number_value().unwrap() < r2.number_value().unwrap(),
                            ),
                        );
                        continue;
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
                    trace.write(FusionInstruction::Cmp(
                        Cmp::Less,
                        vreg!(RegClass::I64, r0),
                        vreg!(RegClass::I64, r1_),
                        vreg!(RegClass::I64, r2_),
                    ));
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
                    let x = !r1.is_false();
                    let y = !r2.is_false();
                    context.set_register(r0, self.allocate_bool(x && y));
                }
                Instruction::BoolOr(r0, r1, r2) => {
                    let r1 = context.get_register(r1);
                    let r2 = context.get_register(r2);
                    let x = !r1.is_false();
                    let y = !r2.is_false();
                    context.set_register(r0, self.allocate_bool(x || y));
                }
                Instruction::LoadNull(x) => context.set_register(x, self.allocate_null()),

                x => panic!("{:?}", x),
            }
        }
    }
}
