use cell::*;
use instruction::*;
use module::*;
use process::*;
use value::*;
use vm::bytecode::*;
use vm::runtime::*;
use vm::util::arc::Arc;
fn main() {
    simple_logger::init().unwrap();
    let mut m = Arc::new(Module::new("Main"));
    let code =
        basicblock::BasicBlock::new(vec![Instruction::LoadInt(0, 42), Instruction::Throw(0)], 0);
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
    RUNTIME.schedule_main_process(proc.clone());
    RUNTIME.start_pools();

    println!("{}", proc.is_terminated());
}
