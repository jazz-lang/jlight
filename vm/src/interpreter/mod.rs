pub mod context;

use crate::heap::*;
use crate::runtime::*;
use crate::util::arc::Arc;
use cell::*;
use context::*;
use gc_pool::Collection;
use process::*;
use scheduler::process_worker::ProcessWorker;
use value::*;
macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident,$bindex: ident) => {{
        $context = $process.context_mut();
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
        reset_context!($proc, $context, $index, $bindex)
    };
}

macro_rules! throw_error_message {
    ($rt: expr,$proc: expr,$msg: expr,$context: ident,$index: ident,$bindex: ident) => {
        let value = $process.allocate_string(&$rt.state, $msg);
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
        Ok(Value::empty())
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
