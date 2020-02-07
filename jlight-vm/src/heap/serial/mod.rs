use crate::runtime::object::*;
use crate::runtime::state::*;
use crate::runtime::threads::JThread;
use crate::runtime::value::*;
use crate::util::shared::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const INITIAL_THRESHOLD: usize = 4096; // 4kb;

// after collection we want the the ratio of used/total to be no
// greater than this (the threshold grows exponentially, to avoid
// quadratic behavior when the heap is growing linearly with the
// number of `new` calls):
const USED_SPACE_RATIO: f64 = 0.7;

pub struct SerialCollector {
    pub heap: Mutex<Vec<ObjectPointer>>,
    pub threshold: AtomicUsize,
    pub bytes_allocated: AtomicUsize,
    pub collecting: AtomicBool,
}
