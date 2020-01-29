use crate::bytecode::*;
use crate::context::*;
use crate::module::*;
use crate::object::*;
use crate::object_value::*;
use crate::process::*;
use crate::state::*;
use crate::sync::*;
const MAIN_MODULE_NAME: &str = "main";

macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident,$bindex: ident) => {{
        $context = $process.context_mut();
        $index = $context.instruction_index;
        $bindex = $context.block_index;
    }};
}

macro_rules! remember_and_reset {
    ($process: expr, $context: ident, $index: ident,$bindex: ident) => {
        $context.instruction_index = $index - 1;
        $context.block_index = $bindex - 1;

        reset_context!($process, $context, $index);
        continue;
    };
}

macro_rules! throw_value {
    (
        $machine:expr,
        $process:expr,
        $value:expr,
        $context:ident,
        $index:ident,
        $bindex: ident
    ) => {{
        $context.instruction_index = $index;
        $context.block_index = $bindex;

        $machine.throw($process, $value)?;

        reset_context!($process, $context, $index);
    }};
}

macro_rules! throw_error_message {
    (
        $machine:expr,
        $process:expr,
        $message:expr,
        $context:ident,
        $index:ident,
        $bindex: ident
    ) => {{
        let value = $process.allocate(
            ObjectValue::String($message),
            $machine.state.string_prototype,
        );

        throw_value!($machine, $process, value, $context, $index, $bindex);
    }};
}

macro_rules! enter_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context.instruction_index = $index;

        reset_context!($process, $context, $index);
    }};
}

macro_rules! safepoint_and_reduce {
    ($vm:expr, $process:expr, $reductions:expr) => {{
        if $vm.gc_safepoint(&$process) {
            return Ok(());
        }

        // Reduce once we've exhausted all the instructions in a
        // context.
        if $reductions > 0 {
            $reductions -= 1;
        } else {
            $vm.state.scheduler.schedule($process.clone());
            return Ok(());
        }
    }};
}

macro_rules! try_runtime_error {
    ($expr:expr, $vm:expr, $proc:expr, $context:ident, $index:ident) => {{
        // When an operation would block, the socket is already registered, and
        // the process may already be running again in another thread. This
        // means that when a WouldBlock is produced it is not safe to access any
        // process data.
        //
        // To ensure blocking operations are retried properly, we _first_ set
        // the instruction index, then advance it again if it is safe to do so.
        $context.instruction_index = $index - 1;

        match $expr {
            Ok(thing) => {
                $context.instruction_index = $index;

                thing
            }
            Err(RuntimeError::Panic(msg)) => {
                $context.instruction_index = $index;

                return Err(msg);
            }
            Err(RuntimeError::Exception(msg)) => {
                throw_error_message!($vm, $proc, msg, $context, $index);
                continue;
            }
            Err(RuntimeError::WouldBlock) => {
                // *DO NOT* use "$context" at this point, as it may have been
                // invalidated if the process is already running again in
                // another thread.
                return Ok(());
            }
        }
    }};
}

#[derive(Clone)]
pub struct Machine {
    pub state: RcState,
    pub module_registry: RcModuleRegistry,
}

impl Machine {
    pub fn throw(&self, process: &RcProcess, value: ObjectPointer) -> Result<(), String> {
        loop {
            let context = process.context_mut();
            if let Some(entry) = process.catch_entries_mut().pop() {
                context.block_index = entry.jump_to as _;
                context.set_register(entry.register, value);

                return Ok(());
            }

            if process.pop_context() {
                return Err(format!(
                    "A thrown value reached top-level in process {:#x}",
                    process.identifier()
                ));
            }
        }
    }

    pub fn run(
        &self,
        worker: &mut crate::scheduler::proc_worker::ProcessWorker,
        process: &RcProcess,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions;
        let mut context: &mut Context;
        let mut index;
        let mut bindex;
        let mut instruction;

        reset_context!(process, context, index, bindex);

        'exec_loop: loop {
            let block: &BasicBlock = unsafe { context.code.get_unchecked(bindex) };
            instruction = block.instructions[index].clone();
            index += 1;
            if index >= block.instructions.len() {
                bindex += 1;
            }

            match instruction {
                Instruction::LoadInt(r, value) => {
                    context
                        .set_register(r, ObjectPointer::integer(f64::to_bits(value as f64) as i64));
                }
                Instruction::LoadNum(r, value) => {
                    context.set_register(r, ObjectPointer::integer(value as _));
                }
                Instruction::Move(to, from) => {
                    context.set_register(to, context.get_register(from));
                }
                Instruction::CatchBlock(r, jump_to) => {
                    process.catch_entries_mut().push(CatchEntry {
                        jump_to,
                        register: r,
                    });
                }
                Instruction::Goto(to) => {
                    bindex = to as _;
                }
                Instruction::Return(r) => {
                    if context.terminate_upon_return {
                        break 'exec_loop;
                    }
                    let object = context.get_register(r);
                    if let Some(register) = context.return_register {
                        if let Some(parent_context) = &mut context.parent {
                            parent_context.set_register(register, object);
                        }
                    }

                    if process.pop_context() {
                        break 'exec_loop;
                    }

                    reset_context!(process, context, index, bindex);
                    safepoint_and_reduce!(self, process, reductions);
                }
                Instruction::Safepoint => {
                    safepoint_and_reduce!(self, process, reductions);
                }
                _ => unimplemented!(),
            }
        }

        Ok(())
    }

    fn gc_safepoint(&self, process: &RcProcess) -> bool {
        if !process.should_collect_young_generation() {
            return false;
        }
        // TODO: Trigger GC
        true
    }
}

pub struct CatchEntry {
    pub register: u16,
    pub jump_to: u16,
}
