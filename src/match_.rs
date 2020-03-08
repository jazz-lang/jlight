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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Complete,
    Partial,
    Dubious,
}

pub enum MatchOp {
    Root,
    Failure,
    Handle(Box<MatchOp>, Box<MatchOp>),
    Constants(Box<MatchOp>, Vec<(Box<Expr>, Box<MatchOp>)>),
    Field(Box<MatchOp>, i32),
    Array(Box<MatchOp>, i32),
    Token(Box<MatchOp>, i32),
    RecordField(Box<MatchOp>, String),
    Junk(Box<MatchOp>, i32, Box<MatchOp>),
    Switch(Box<MatchOp>, Vec<(Box<Expr>, Box<MatchOp>)>),
    Bind(String, Box<MatchOp>, Box<MatchOp>),
    When(Box<Expr>, Box<MatchOp>),
    Next(Box<MatchOp>, Box<MatchOp>),
}

use crate::ast::*;
use crate::msg::*;
