extern crate jlightc as jlight;

use jlight::codegen::compile;
use jlight::parser::*;
use jlight::reader::*;
use jlight_vm::runtime::*;
fn main() {
    let mut ast = vec![];
    let r = Reader::from_string(
        "


function Point(x,y) {
    self.x = x
    self.y = y
    return self
}

var p = new Point(2,3)
io.writeln(p.x)
",
    );
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let mut ctx = compile(ast);
    ctx.finalize();
    let state = jlight_vm::util::arc::Arc::new(jlight_vm::runtime::state::State::new());
    let module = jlight::codegen::module_from_ctx(&ctx, &state);
    let rt = Runtime::new();
    jlight::codegen::disassemble_module(&module);
    rt.run_function(module.globals.get().last().unwrap().clone());
}
