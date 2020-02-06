extern crate jlightc as jlight;

use jlight::codegen::compile;
use jlight::parser::*;
use jlight::reader::*;
use jlight_vm::runtime::*;
fn main() {
    let start_time = std::time::Instant::now();
    let mut ast = vec![];
    let r = Reader::from_string(
        " 

var x = 42

var y = 3
var z = x + y
return z
",
    );
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let mut ctx = compile(ast);
    ctx.finalize();
    let state = jlight_vm::util::shared::Arc::new(jlight_vm::runtime::state::State::new());
    let module = jlight::codegen::module_from_ctx(&ctx, &state);
    jlight::codegen::disassemble_module(&module);
    let execution_time = std::time::Instant::now();
    RUNTIME.state.threads.attach_current_thread();
    //RUNTIME.run_function(module.globals.get().last().unwrap().clone());
    let exec = execution_time.elapsed();
    let end = start_time.elapsed();
    println!(
        "Program done in {}ms ({}ns) and code executed in {}ms ({}ns)",
        end.as_millis(),
        end.as_nanos(),
        exec.as_millis(),
        exec.as_nanos()
    )
}
