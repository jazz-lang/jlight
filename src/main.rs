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
use std::io::Write;
use std::path::PathBuf;
use structopt::StructOpt;
use value::*;
use waffle::bytecode::*;
use waffle::runtime::*;
use waffle::util::arc::Arc;
use writer::BytecodeWriter;
#[derive(Debug, StructOpt)]
#[structopt(name = "jazzlight", about = "Compiler")]
struct Opt {
    #[structopt(name = "FILE", parse(from_os_str))]
    input: PathBuf,

    #[structopt(
        parse(from_os_str),
        long = "output",
        short = "o",
        default_value = "a.out"
    )]
    output: PathBuf,
}

fn main() {
    simple_logger::init().unwrap();
    let opt: Opt = Opt::from_args();
    let mut ast = vec![];
    let r = Reader::from_file(opt.input.to_str().unwrap()).unwrap();
    let mut p = Parser::new(r, &mut ast);
    p.parse().unwrap();
    let mut m = compile(ast);
    m.finalize();
    let mut module = module_from_ctx(&m);
    disassemble_module(&module);
    let mut writer = BytecodeWriter { bytecode: vec![] };
    writer.write_module(&mut module);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(opt.output)
        .unwrap();
    f.set_len(0).unwrap();
    f.write_all(&writer.bytecode).unwrap();
}
