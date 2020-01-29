use super::block::*;
use super::block_list::*;
use super::global_allocator::*;
use super::histograms::*;
use crate::object::*;
use crate::ptr::*;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
/// The age of a bucket containing mature objects.
pub const MATURE: i8 = 125;

/// The age of a bucket containing mailbox objects.
pub const MAILBOX: i8 = 126;

/// The age of a bucket containing permanent objects.
pub const PERMANENT: i8 = 127;

macro_rules! lock_bucket {
    ($bucket: expr) => {
        unsafe { &*$bucket.lock.get() }.lock()
    };
}
/// Structure storing data of a single bucket.
pub struct Bucket {
    /// Lock used whenever moving objects around (e.g. when evacuating or
    /// promoting them).
    pub lock: UnsafeCell<Mutex<()>>,

    /// The blocks managed by this bucket.
    pub blocks: BlockList,

    /// The current block to allocate into.
    ///
    /// This pointer may be NULL to indicate no block is present yet.
    pub current_block: Ptr<Block>,

    /// The age of the objects in the current bucket.
    pub age: i8,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Self {
        Self::with_age(0)
    }

    pub fn with_age(age: i8) -> Self {
        Bucket {
            blocks: BlockList::new(),
            current_block: Ptr::null(),
            age,
            lock: UnsafeCell::new(Mutex::new(())),
        }
    }

    pub fn reset_age(&mut self) {
        self.age = 0;
    }

    pub fn increment_age(&mut self) {
        self.age += 1;
    }

    pub fn current_block(&self) -> Option<Ptr<Block>> {
        let pointer = self.current_block.atomic_load();

        if pointer.is_null() {
            None
        } else {
            Some(pointer)
        }
    }

    pub fn has_current_block(&self) -> bool {
        self.current_block().is_some()
    }

    pub fn set_current_block(&mut self, block: Ptr<Block>) {
        self.current_block.atomic_store(block.0);
    }

    pub fn add_block(&mut self, mut block: Box<Block>) {
        block.set_bucket(self as *mut Bucket);

        self.set_current_block(Ptr::from_ref(&*block));
        self.blocks.push(block);
    }

    pub fn reset_current_block(&mut self) {
        self.set_current_block(self.blocks.head());
    }

    /// Allocates an object into this bucket
    ///
    /// The return value is a tuple containing a boolean that indicates if a new
    /// block was requested, and the pointer to the allocated object.
    ///
    /// This method can safely be used concurrently by different threads.
    pub fn allocate(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let mut new_block = false;

        loop {
            let mut advance_block = false;
            let started_at = self.current_block.atomic_load();

            if let Some(current) = self.current_block() {
                for mut block in current.iter() {
                    if block.is_fragmented() {
                        // The block is fragmented, so skip it. The next time we
                        // find an available block we'll set it as the current
                        // block.
                        advance_block = true;

                        continue;
                    }

                    if let Some(raw_pointer) = block.request_pointer() {
                        if advance_block {
                            let _lock = lock_bucket!(self);

                            // Only advance the block if another thread didn't
                            // request a new one in the mean time.
                            self.current_block
                                .compare_and_swap(started_at.0, &mut *block);
                        }

                        return (new_block, object.write_to(raw_pointer));
                    }
                }
            }

            // All blocks have been exhausted, or there weren't any to begin
            // with. Let's request a new one, if still necessary after obtaining
            // the lock.
            let _lock = lock_bucket!(self);

            if started_at == self.current_block.atomic_load() {
                new_block = true;
                self.add_block(global_allocator.request_block());
            }
        }
    }
    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    ///
    /// The return value is the total number of blocks after reclaiming
    /// finishes.
    pub fn reclaim_blocks(
        &mut self,
        state: &crate::state::State,
        histograms: &mut Histograms,
    ) -> usize {
        let mut to_release = BlockList::new();
        let mut amount = 0;

        // We perform this work sequentially, as performing this in parallel
        // would require multiple passes over the list of input blocks. We found
        // that performing this work in parallel using Rayon ended up being
        // about 20% slower, likely due to:
        //
        // 1. The overhead of distributing work across threads.
        // 2. The list of blocks being a linked list, which can't be easily
        //    split to balance load across threads.
        for mut block in self.blocks.drain() {
            block.update_line_map();

            if block.is_empty() {
                block.reset();
                to_release.push(block);
            } else {
                let holes = block.update_hole_count();

                // Clearing the fragmentation status is done so a block does
                // not stay fragmented until it has been evacuated entirely.
                // This ensures we don't keep evacuating objects when this
                // may no longer be needed.
                block.clear_fragmentation_status();

                if holes > 0 {
                    if holes >= MINIMUM_BIN {
                        histograms
                            .marked
                            .increment(holes, block.marked_lines_count() as u32);
                    }

                    block.recycle();
                }

                amount += 1;

                self.blocks.push(block);
            }
        }

        state.global_allocator.add_blocks(&mut to_release);

        self.reset_current_block();

        amount
    }

    /// Prepares this bucket for a collection.
    pub fn prepare_for_collection(&mut self, histograms: &mut Histograms, evacuate: bool) {
        let mut required: isize = 0;
        let mut available: isize = 0;

        for block in self.blocks.iter_mut() {
            let holes = block.holes();

            // We ignore blocks with only a single hole, as those are not
            // fragmented and not worth evacuating. This also ensures we ignore
            // blocks added since the last collection, which will have a hole
            // count of 1.
            if evacuate && holes >= MINIMUM_BIN {
                let lines = block.available_lines_count() as u32;

                histograms.available.increment(holes, lines);

                available += lines as isize;
            };

            // We _must_ reset the bytemaps _after_ calculating the above
            // statistics, as those statistics depend on the mark values in
            // these maps.
            block.prepare_for_collection();
        }

        if available > 0 {
            let mut min_bin = 0;
            let mut bin = MAX_HOLES;

            // Bucket 1 refers to blocks with only a single hole. Blocks with
            // just one hole aren't fragmented, so we ignore those here.
            while available > required && bin >= MINIMUM_BIN {
                required += histograms.marked.get(bin) as isize;
                available -= histograms.available.get(bin) as isize;

                min_bin = bin;
                bin -= 1;
            }

            if min_bin > 0 {
                for block in self.blocks.iter_mut() {
                    if block.holes() >= min_bin {
                        block.set_fragmented();
                    }
                }
            }
        }
    }
}
