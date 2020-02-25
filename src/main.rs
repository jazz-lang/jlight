/*
*   Copyright (c) 2020 Adel Prokurov
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/

extern crate jlightc as jlight;

use cell::*;
use instruction::*;
use jlight::codegen::*;
use jlight::parser::*;
use jlight::reader::*;
use module::*;
use process::*;
use value::*;
use waffle::bytecode::*;
use waffle::runtime::*;
use waffle::util::arc::Arc;
fn main() {
    simple_logger::init().unwrap();
    let start_time = std::time::Instant::now();
    let mut ast = vec![];
    let r = Reader::from_string(
        "
var i = 0
if false {
    io.writeln(1)
} else if true {
    io.writeln(3)
} else {
    io.writeln(2)
}
",
    );
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let mut m = compile(ast);
    m.finalize();
    let module = module_from_ctx(&m);
    disassemble_module(&module);
    let proc = Process::from_function(
        module.globals.last().map(|x| *x).unwrap(),
        &config::Config::default(),
    )
    .unwrap();
    println!("Scheduling process...");
    RUNTIME.schedule_main_process(proc.clone());
    RUNTIME.start_pools();
    //RUNTIME.state.gc.major_collection(&RUNTIME.state);
}
