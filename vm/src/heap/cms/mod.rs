//! Concurrent Mark&Sweep collector

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
pub mod node_pool;
pub mod stub;

pub struct CMS {
    reg_mut: Mutex<()>,
}
