extern crate jlightc as jlight;

use jlight::codegen::compile;
use jlight::parser::*;
use jlight::reader::*;

fn main() {
    let mut ast = vec![];
    let r = Reader::from_string(
        "
function foo() {}
var x = 42
var y = x
var z = y
return y
",
    );
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let ctx = compile(ast);

    for (i, bb) in ctx.bbs.iter().enumerate() {
        println!("{}:", i);
        for ins in bb.instructions.iter() {
            println!("  {:?}", ins);
        }
    }
    let mut state = jlight_vm::util::arc::Arc::new(jlight_vm::runtime::state::State::new());
    let module = jlight::codegen::module_from_ctx(&ctx, &state);
}
