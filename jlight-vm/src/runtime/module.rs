use super::state::*;
use super::value::*;
use crate::util::ptr::Ptr;
use fxhash::FxHashMap;

pub struct Module {
    pub globals: Ptr<Vec<Value>>,
}

impl Module {
    pub fn new() -> Self {
        Self {
            globals: Ptr::null(),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        if self.globals.is_null() {
            return;
        }
        unsafe {
            let _ = Box::from_raw(self.globals.0);
        }
    }
}

pub struct ModuleRegistry {
    state: RcState,
    pub parsed: FxHashMap<String, Value>,
}

impl ModuleRegistry {
    pub fn new(state: RcState) -> Self {
        Self {
            state,
            parsed: FxHashMap::with_hasher(fxhash::FxBuildHasher::default()),
        }
    }
}

impl Drop for ModuleRegistry {
    fn drop(&mut self) {}
}
