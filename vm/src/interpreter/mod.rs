pub mod context;

use crate::bytecode::instruction::*;
use crate::heap::*;
use crate::runtime::*;
use crate::util::arc::Arc;
use crate::util::ptr::Ptr;
use cell::*;
use context::*;
use gc_pool::Collection;
use process::*;
use scheduler::process_worker::ProcessWorker;
use value::*;

macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident,$bindex: ident) => {{
        $context = $process.context_ptr();
        $index = $context.index;
        $bindex = $context.bindex;
    }};
}

macro_rules! remember_and_reset {
    ($process: expr, $context: ident, $index: ident,$bindex: ident) => {
        $context.index = $index - 1;

        reset_context!($process, $context, $index, $bindex);
        continue;
    };
}

macro_rules! throw {
    ($rt: expr,$proc: expr,$value: expr,$context: ident,$index: ident, $bindex: ident) => {
        $context.index = $index;
        $context.bindex = $bindex;
        $rt.throw($proc, $value)?;
        reset_context!($proc, $context, $index, $bindex);
        continue;
    };
}

macro_rules! throw_error_message {
    ($rt: expr,$proc: expr,$msg: expr,$context: ident,$index: ident,$bindex: ident) => {
        let value = $proc.allocate_string(&$rt.state, $msg);
        throw!($rt, $proc, value, $context, $index, $bindex)
    };
}

macro_rules! enter_context {
    ($process: expr,$context: ident,$index: ident,$bindex: ident) => {
        $context.bindex = $bindex;
        $context.index = $index;
        reset_context!($process, $context, $index, $bindex);
    };
}

macro_rules! safepoint_and_reduce {
    ($rt: expr,$process: expr,$reductions: expr) => {
        if $rt.gc_safepoint($process) {
            return Ok(Value::from(VTag::Null));
        }

        if $reductions > 0 {
            $reductions -= 1;
        } else {
            $rt.state.scheduler.schedule($process.clone());
            return Ok(Value::from(VTag::Null));
        }
    };
}

impl Runtime {
    pub fn run(&self, worker: &mut ProcessWorker, process: &Arc<Process>) -> Result<Value, Value> {
        let mut reductions = 1000;
        let mut index;
        let mut bindex;
        let mut context: Ptr<Context>;
        reset_context!(process, context, index, bindex);

        loop {
            let block = unsafe { context.code.get_unchecked(bindex) };
            let ins = unsafe { block.instructions.get_unchecked(index) };
            index += 1;
            match *ins {
                Instruction::Return(value) => {
                    let value = if let Some(value) = value {
                        context.get_register(value)
                    } else {
                        Value::from(VTag::Null)
                    };
                    self.clear_catch_tables(&context, process);
                    if context.terminate_upon_return {
                        return Ok(value);
                    }

                    if process.pop_context() {
                        return Ok(value);
                    }
                    reset_context!(process, context, index, bindex);
                    safepoint_and_reduce!(self, process, reductions);
                }
                Instruction::LoadNull(r) => context.set_register(r, Value::from(VTag::Null)),
                Instruction::LoadUndefined(r) => {
                    context.set_register(r, Value::from(VTag::Undefined))
                }
                Instruction::LoadInt(r, i) => context.set_register(r, Value::new_int(i)),
                Instruction::LoadNumber(r, f) => {
                    context.set_register(r, Value::new_double(f64::from_bits(f)))
                }
                Instruction::LoadTrue(r) => context.set_register(r, Value::from(VTag::True)),
                Instruction::LoadFalse(r) => context.set_register(r, Value::from(VTag::False)),
                Instruction::LoadById(to, obj, id) => {
                    let id = context.module.globals[id as usize].to_string();
                    let obj = context.get_register(obj);
                    let field = obj.lookup_attribute(&self.state, &Arc::new(id.clone()));
                    if field.is_none() {
                        throw_error_message!(
                            self,
                            process,
                            &format!("Field '{}' not found", id),
                            context,
                            index,
                            bindex
                        );
                    }
                    context.set_register(to, field.unwrap());
                }
                Instruction::Push(r) => {
                    let value = context.get_register(r);
                    context.stack.push(value);
                }
                Instruction::Pop(r) => {
                    let value = context.stack.pop().unwrap_or(Value::from(VTag::Undefined));
                    context.set_register(r, value);
                }
                Instruction::Branch(block) => bindex = block as usize,
                Instruction::ConditionalBranch(r, if_true, if_false) => {
                    let value = context.get_register(r);
                    if value.to_boolean() {
                        bindex = if_true as _;
                    } else {
                        bindex = if_false as _;
                    }
                }
                Instruction::MakeEnv(function, count) => {
                    let mut upvalues = vec![];
                    for _ in 0..count {
                        upvalues.push(context.stack.pop().unwrap());
                    }

                    let function = context.get_register(function);
                    if function.is_cell() {
                        match function.as_cell().get_mut().value {
                            CellValue::Function(ref mut f) => f.upvalues = upvalues,
                            _ => {
                                /*throw_error_message!(
                                    self,
                                    process,
                                    &format!(
                                        "MakeEnv: Function expected, found '{}'",
                                        function.to_string()
                                    ),
                                    context,
                                    index,
                                    bindex
                                );*/
                                panic!(
                                    "MakeEnv: Function expected, found '{}'",
                                    function.to_string()
                                );
                            }
                        }
                    } else {
                        panic!(
                            "MakeEnv: Function expected, found '{}'",
                            function.to_string()
                        );
                    }
                }
                _ => unimplemented!(),
            }
        }
    }
    /// Returns true if a process should be suspended for garbage collection.
    pub fn gc_safepoint(&self, process: &Arc<Process>) -> bool {
        if process.local_data().heap.needs_gc != GCType::Young {
            return false;
        }
        self.state
            .gc_pool
            .schedule(Collection::new(process.clone()));
        true
    }
    pub fn throw(&self, process: &Arc<Process>, value: Value) -> Result<Value, Value> {
        if let Some(table) = process.local_data_mut().catch_tables.pop() {
            let mut catch_ctx = table.context.replace(Context::new());
            catch_ctx.set_register(table.register, value);
            catch_ctx.bindex = table.jump_to as _;
            process.push_context(catch_ctx);

            Ok(Value::empty())
        } else {
            return Err(value);
        }
    }
    pub fn clear_catch_tables(&self, exiting: &Ptr<Context>, proc: &Arc<Process>) {
        proc.local_data_mut()
            .catch_tables
            .retain(|ctx| ctx.context.n < exiting.n);
    }
    pub fn run_default_panic(&self, proc: &Arc<Process>, message: &Value) {
        runtime_panic(proc, message);
        self.terminate();
    }

    pub fn terminate(&self) {
        self.state.scheduler.terminate();
        self.state.gc_pool.terminate();
        self.state.timeout_worker.terminate();
    }
}

pub fn runtime_panic(process: &Arc<Process>, message: &Value) {
    let mut frames = vec![];
    let mut buffer = String::new();
    for ctx in process.local_data().context.contexts() {
        frames.push(format!(
            "\"{}\" in {}",
            ctx.module.name,
            ctx.function.function_value().unwrap().name
        ));
    }

    frames.reverse();
    buffer.push_str("Stack trace (the most recent call comes last):");

    for (index, line) in frames.iter().enumerate() {
        buffer.push_str(&format!("\n  {}: {}", index, line));
    }

    buffer.push_str(&format!(
        "\nProcess {:#x} panicked: {}",
        process.as_ptr() as usize,
        message.to_string(),
    ));

    eprintln!("{}", buffer);
}
