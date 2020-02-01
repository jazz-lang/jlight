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
var m1 = new Array(
    new Array(1,2,3),
    new Array(4,5,6),
    new Array(7,8,9)
)

var m2 = new Array(
    new Array(10,11,12),
    new Array(13,14,15),
    new Array(16,17,18)
)

var m3 = new Array(
    new Array(m1[0][0] * m2[0][0],m1[0][1] * m2[0][1],m1[0][2] * m2[0][2]),
    new Array(m1[1][0] * m2[1][0],m1[0][1] * m2[1][1],m1[0][2] * m2[1][2]),
    new Array(m1[2][0] * m2[2][0],m1[0][1] * m2[2][1],m1[0][2] * m2[2][2]),
)
var m4 = new Array(
    new Array(m3[0][0] * m2[0][0],m3[0][1] * m2[0][1],m3[0][2] * m2[0][2]),
    new Array(m3[1][0] * m2[1][0],m3[0][1] * m2[1][1],m3[0][2] * m2[1][2]),
    new Array(m3[2][0] * m2[2][0],m3[0][1] * m2[2][1],m3[0][2] * m2[2][2]),
)


io.writeln(m4)
",
    );
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let mut ctx = compile(ast);
    ctx.finalize();
    let state = jlight_vm::util::arc::Arc::new(jlight_vm::runtime::state::State::new());
    let module = jlight::codegen::module_from_ctx(&ctx, &state);
    jlight::codegen::disassemble_module(&module);
    let execution_time = std::time::Instant::now();
    RUNTIME.state.threads.attach_current_thread();
    RUNTIME.run_function(module.globals.get().last().unwrap().clone());
    let end = start_time.elapsed();
    let exec = execution_time.elapsed();
    println!(
        "Program done in {}ms ({}ns) and code executed in {}ms ({}ns)",
        end.as_millis(),
        end.as_nanos(),
        exec.as_millis(),
        exec.as_nanos()
    )
}
