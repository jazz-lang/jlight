use super::*;
use crate::bytecode::block::*;
use crate::bytecode::instructions::*;
use crate::util;
use context::*;
use object::*;
use threads::*;
use util::{arc::Arc, ptr::Ptr};
use value::*;
static TABLE: [fn(&mut State<'_>, Instruction) -> Value; 1] = [load_int];

pub struct State<'a> {
    rt: &'a Runtime,
    thread: &'a Arc<JThread>,
    context: Ptr<Context>,
    bindex: usize,
    index: usize,
}

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

impl<'a> State<'a> {
    /// This function **must** be tail call otherwise we will get stack overflow.
    /// Though we can use inline assembly to implement threaded interpreter but this requires
    /// lots of unsafe and nightly Rust.
    #[inline(always)]
    pub fn dispatch(&mut self) -> Value {
        let block = unsafe { self.context.code.get_unchecked(self.bindex) };
        let instruction = unsafe { block.instructions.get_unchecked(self.index).clone() };
        self.index += 1;
        TABLE[instruction.discriminant()](self, instruction)
    }
}

/// Just hint to compiler that this code is *really* unreachable.
fn unreachable() -> ! {
    unsafe {
        std::hint::unreachable_unchecked();
    }
}

macro_rules! ins {
    ($ins: expr; $name: ident ($($arg: ident),*) $b: block) => {
        match $ins {
            Instruction::$name ($($arg),*) => {
                $b
            }
            _ => unreachable()
        }
    };
}

fn load_int(state: &mut State<'_>, instruction: Instruction) -> Value {
    ins!(
        instruction; LoadInt(r,i) {
            state.context.set_register(r, unimplemented!())
        }
    );
    state.dispatch()
}
