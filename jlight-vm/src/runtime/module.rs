use super::object::*;
use super::state::*;
use crate::bytecode::block::BasicBlock;
use crate::util::deref_ptr::DerefPointer;
use crate::util::ptr::Ptr;
use fxhash::FxHashMap;

pub struct Module {
    pub globals: Ptr<Vec<ObjectPointer>>,
    pub labels: FxHashMap<u16, DerefPointer<BasicBlock>>,
}

impl Module {
    pub fn new() -> Self {
        Self {
            globals: Ptr::null(),
            labels: FxHashMap::with_hasher(fxhash::FxBuildHasher::default()),
        }
    }
}

pub struct ModuleRegistry {
    state: RcState,
    parsed: FxHashMap<String, ObjectPointer>,
}
