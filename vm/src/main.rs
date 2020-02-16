use cell::*;
use instruction::*;
use module::*;
use process::*;
use value::*;
use waffle::bytecode::*;
use waffle::runtime::*;
use waffle::util::arc::Arc;
fn main() {
    simple_logger::init().unwrap();
    let mut m = Arc::new(Module::new("Main"));
    let code = basicblock::BasicBlock::new(vec![Instruction::Return(None)], 0);
    let func = Function {
        upvalues: vec![],
        name: Arc::new("main".to_owned()),
        module: m.clone(),
        code: Arc::new(vec![code]),
        native: None,
        argc: 0,
    };
    let value = RUNTIME.state.allocate_fn(func);
    let proc = Process::from_function(value, &config::Config::default()).unwrap();
    let s = proc.allocate_string(&RUNTIME.state, "Wooooow!");
    m.globals.push(s);
    RUNTIME.schedule_main_process(proc.clone());
    RUNTIME.start_pools();

    println!("{}", proc.is_terminated());
    m.globals.pop();
    proc.do_gc();
}
