use super::block::Block;
use super::block_list::BlockList;
use crate::sync::Arc;
use parking_lot::Mutex;
pub type RcGlobalAllocator = Arc<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    blocks: Mutex<BlockList>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn with_rc() -> RcGlobalAllocator {
        Arc::new(GlobalAllocator {
            blocks: Mutex::new(BlockList::new()),
        })
    }

    /// Requests a new free block from the pool
    pub fn request_block(&self) -> Box<Block> {
        if let Some(block) = self.blocks.lock().pop() {
            block
        } else {
            Block::boxed()
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Box<Block>) {
        self.blocks.lock().push(block);
    }

    /// Adds multiple blocks to the global allocator.
    pub fn add_blocks(&self, to_add: &mut BlockList) {
        let mut blocks = self.blocks.lock();

        blocks.append(to_add);
    }
}
