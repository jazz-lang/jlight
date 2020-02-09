use super::value::*;
use crate::util::arc::Arc;
use alloc::vec::Vec;
pub struct Module {
    globals: Vec<Value>,
}
