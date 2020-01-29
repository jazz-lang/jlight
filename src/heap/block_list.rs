use super::block::*;
use super::map::*;
use crate::ptr::Ptr;
use std::mem;
use std::ops::Index;
use std::slice::IterMut as SliceIterMut;
use std::vec::IntoIter as VecIntoIter;
/// A linked list of blocks.
#[cfg_attr(feature = "cargo-clippy", allow(clippy::vec_box))]
pub struct BlockList {
    /// The blocks managed by this BlockList. Each Block also has its "next"
    /// pointer set, allowing allocators to iterate the list while it may be
    /// modified.
    blocks: Vec<Box<Block>>,
}

/// An iterator over block pointers.
pub struct BlockIterator {
    current: Ptr<Block>,
}

/// An iterator over owned block pointers.
pub struct Drain {
    blocks: VecIntoIter<Box<Block>>,
}

impl BlockList {
    pub fn new() -> Self {
        BlockList { blocks: Vec::new() }
    }

    /// Pushes a block to the start of the list.
    pub fn push(&mut self, block: Box<Block>) {
        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(Ptr::from_ref(&*block));
        }

        self.blocks.push(block);
    }

    /// Pops a block from the start of the list.
    pub fn pop(&mut self) -> Option<Box<Block>> {
        let block = self.blocks.pop();

        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(Ptr::null());
        }

        block
    }

    /// Adds the other list to the end of the current list.
    pub fn append(&mut self, other: &mut Self) {
        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(other.head());
        }

        self.blocks.append(&mut other.blocks);
    }

    /// Counts the number of blocks in this list.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn head(&self) -> Ptr<Block> {
        if let Some(block) = self.blocks.first() {
            Ptr::from_ref(&**block)
        } else {
            Ptr::null()
        }
    }

    /// Returns an iterator that iterates over the Vec, instead of using the
    /// "next" pointers of every block.
    pub fn iter_mut<'a>(&'a mut self) -> SliceIterMut<'a, Box<Block>> {
        self.blocks.iter_mut()
    }

    /// Returns an iterator that yields owned block pointers.
    ///
    /// Calling this method will reset the head and tail. The returned iterator
    /// will consume all blocks.
    pub fn drain(&mut self) -> Drain {
        let mut blocks = Vec::new();

        mem::swap(&mut blocks, &mut self.blocks);

        Drain {
            blocks: blocks.into_iter(),
        }
    }
}

impl Index<usize> for BlockList {
    type Output = Block;

    /// Returns a reference to the block at the given index.
    fn index(&self, index: usize) -> &Self::Output {
        &self.blocks[index]
    }
}

impl BlockIterator {
    /// Creates a new iterator starting at the given block.
    pub fn starting_at(block: &Block) -> Self {
        BlockIterator {
            current: Ptr::from_ref(block),
        }
    }
}

impl Iterator for BlockIterator {
    type Item = Ptr<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            None
        } else {
            let current = Some(self.current);

            // One thread may be iterating (e.g. when allocating into a bucket)
            // when the other thread is adding a block. When reaching the end of
            // the list, without an atomic load we may (depending on the
            // platform) read an impartial or incorrect value.
            self.current = self.current.header().next.atomic_load();

            current
        }
    }
}

impl Iterator for Drain {
    type Item = Box<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(mut block) = self.blocks.next() {
            block.header_mut().set_next(Ptr::null());

            Some(block)
        } else {
            None
        }
    }
}
