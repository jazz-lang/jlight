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

use jlight::codegen::*;
use jlight::parser::*;
use jlight::reader::*;
use std::io::Write;
use std::path::PathBuf;
use structopt::StructOpt;
use waffle::bytecode::passes::simple_inlining::*;
use waffle::bytecode::*;
use waffle::runtime::cell::*;
use writer::BytecodeWriter;
#[derive(Debug, StructOpt)]
#[structopt(name = "jlightc", about = "Compiler")]
struct Opt {
    #[structopt(
        long = "no-std",
        help = "Do not invoke __start__ function from Waffle runtime to load core modules"
    )]
    no_std: bool,

    #[structopt(name = "FILE", parse(from_os_str))]
    input: PathBuf,
    #[structopt(parse(from_os_str), long, short, default_value = "program.wfl")]
    output: PathBuf,
}

fn main() {
    let opt: Opt = Opt::from_args();
    simple_logger::init().unwrap();
    let mut ast = vec![];
    let no_std = std::env::var("NO_STD_BUILD").is_ok();
    let r = Reader::from_file(opt.input.to_str().unwrap()).unwrap();
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let m = compile(ast, no_std || opt.no_std);
    let mut m = if let Ok(c) = m {
        c
    } else {
        eprintln!("{}", m.err().unwrap());
        std::process::exit(1);
    };
    m.finalize(false, "main".to_owned());
    let mut module = module_from_ctx(&m);
    println!("before optimizations: ");
    disassemble_module(&module);
    prelink_module(&module, OptLevel::Fast);
    println!("after:");
    disassemble_module(&module);
    let mut writer = BytecodeWriter { bytecode: vec![] };
    writer.write_module(&mut module);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open("program.wfl")
        //.open(opt.output)
        .unwrap();
    f.set_len(0).unwrap();
    f.write_all(&writer.bytecode).unwrap();
}
