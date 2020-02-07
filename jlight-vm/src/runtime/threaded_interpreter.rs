use super::*;
use crate::bytecode::block::*;
use crate::bytecode::instructions::*;
use crate::util;
use context::*;
use object::*;
use threads::*;
use util::{arc::Arc, ptr::Ptr};
use value::*;
static TABLE: [fn(&mut State<'_>, Instruction) -> Result<Value, Value>; 5] =
    [load, store, load_int, load_null, load_num];

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
    pub fn dispatch(&mut self) -> Result<Value, Value> {
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
    ($fname: ident $name: ident ($($arg: ident),*) $state: ident, $b: block) => {
        fn $fname (state: &mut State<'_>,i: Instruction) -> Result<Value,Value> {
            match i {
                Instruction::$name ($($arg),*) => {
                    let $state = state;
                    $b
                }
                _ => unreachable()
            }
        }
    };
}
/*
fn load_int(state: &mut State<'_>, instruction: Instruction) -> Result<Value, Value> {
    ins!(
        instruction; LoadInt(r,i) {
            state.context.set_register(r, Value::new_double(i as i64 as f64));

        }
    );
    state.dispatch()
}*/

ins!(
    load_int LoadInt(r,i) state, {
        state.context.set_register(r,Value::new_double(i as i64 as f64));
        state.dispatch()
    }
);

ins!(
    load_num LoadNum(r,f) state, {
        state.context.set_register(r,Value::new_double(f64::from_bits(f)));
        state.dispatch()
    }
);

ins!(
    load Load(to,object,key) state, {
        let object = state.context.get_register(object);
        let key = state.context.get_register(key);
        if object.is_cell() {
            let object = object.as_cell();
            match &mut object.get_mut().value {
                ObjectValue::Array(ref mut array) => {
                    if key.is_number() {
                        let idx = key.to_number() as usize;
                        if idx >= array.len() {
                            for _ in array.len()..=idx {
                                array.push(state.rt.state.nil_prototype)
                            }
                        }
                        state.context
                            .set_register(to, array[key.to_number() as usize]);
                        return state.dispatch();
                    }
                }
                _ => (),
            }
            let key = key.to_string();

            let value = object.lookup_attribute(&state.rt.state, &key);
            state.context.set_register(to, value.unwrap_or(Value::from(VTag::Null)));
        } else {
            state.context.set_register(to,Value::from(VTag::Null));
        }
        state.dispatch()
    }
);

ins!(
    store Store(object,key,value) state, {
        let object = state.context.get_register(object);
        let key = state.context.get_register(key);
        let value = state.context.get_register(value);
        if object.is_cell() {
            if let ObjectValue::Array(array) = &mut object.as_cell().get_mut().value {
                if key.is_number() {
                    if value.is_cell() {
                        state.rt.state.gc.write_barrier(object.as_cell(),value.as_cell());
                    }
                    array[key.to_number() as usize] = value;
                    return state.dispatch()
                }
            }
        }
        object.add_attribute(&state.rt.state,&key.to_string(),value);
        state.dispatch()
    }
);
ins!(
    load_null LoadNull(r) state, {
        state.context.set_register(r,Value::from(VTag::Null));
        state.dispatch()
    }
);

ins!(
    load_bool LoadBool(r,v) state, {
        state.context.set_register(r,Value::from(if v {VTag::True} else {VTag::False}));
        state.dispatch()
    }
);
