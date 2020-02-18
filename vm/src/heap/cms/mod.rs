//! Concurrent Mark&Sweep collector
//!
//! This GC performs small pauses to trace roots and then resumes process execution and does all work in background thread
//!
//!

use crate::util::mem::Address;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
pub mod atomic_list;
pub mod node_pool;
pub mod stub;

pub struct CMS {
    reg_mut: Mutex<()>,
}

pub type UnderlyingHeader = u64;
pub type Header = AtomicU64;
pub type UnderlyingLogPtr = Address;
pub type LogPtr = AtomicUsize;

pub const ZEROED_HEADER: UnderlyingHeader = 0;
pub const ZEROED_LOG_PTR: UnderlyingLogPtr = Address(0);
pub const COLOR_BITS: u64 = 2;
pub const TAG_BITS: u64 = 8;
pub const HEADER_TAG_MASK: u64 = ((1 << TAG_BITS) - 1) << COLOR_BITS;
pub const HEADER_COLOR_MASK: u64 = 0x3;
pub const HEADER_SIZE: usize = std::mem::size_of::<Header>();
pub const LOG_PTR_SIZE: usize = std::mem::size_of::<LogPtr>();
pub const LOG_PTR_OFFSET: usize =
    2 * std::mem::size_of::<usize>() + 2 * std::mem::size_of::<usize>();
pub const SEARCH_DEPTH: usize = 32;
pub const SEGMENT_SIZE: usize = 64;
pub const SMALL_BLOCK_METADATA_SIZE: usize = HEADER_SIZE + LOG_PTR_SIZE;
pub const SMALL_BLOCK_SIZE_LIMIT: usize = 6;
pub const SPLIT_BITS: usize = 32;
pub const SPLIT_MASK: usize = (1usize << SPLIT_BITS) - 1;
pub const SPLIT_SWITCH_BITS: usize = 32;
pub const SPLIT_SWITCH_MASK: usize = (1usize << SPLIT_SWITCH_BITS) - 1 << SPLIT_BITS;
pub const LARGE_BLOCK_METADATA_SIZE: usize =
    2 * std::mem::size_of::<usize>() + HEADER_SIZE + std::mem::size_of::<usize>();
pub const LARGE_OBJ_MIN_BITS: usize = 10;
pub const LARGE_OBJ_THRESHOLD: usize = 1 << (LARGE_OBJ_MIN_BITS - 1);
pub const MARK_TICK_FREQUENCY: usize = 64;
pub const POOL_CHUNK_SIZE: usize = 64;
pub const SMALL_SIZE_CLASSES: usize = 7;
pub const TICK_FREQUENCY: usize = 32;
