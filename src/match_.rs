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
