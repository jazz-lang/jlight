use super::block::*;
use crate::chunk::Chunk;
/// The minimum bin number that we care about when obtaining the most fragmented
/// bins.
///
/// Bins 0 and 1 are not interesting, because blocks with 0 or 1 holes are not
/// used for calculating fragmentation statistics.
pub const MINIMUM_BIN: usize = 2;

pub struct Histogram {
    // We use a u32 as this allows for 4 294 967 295 lines per bucket, which
    // equals roughly 512 GB of lines.
    values: Chunk<u32>,
}

impl Histogram {
    pub fn new(capacity: usize) -> Self {
        let values = Chunk::new(capacity);

        Histogram { values }
    }

    /// Increments a bin by the given value.
    ///
    /// Bounds checking is not performed, as the garbage collector never uses an
    /// out of bounds index.
    pub fn increment(&mut self, index: usize, value: u32) {
        debug_assert!(index < self.values.len());

        self.values[index] += value;
    }

    /// Returns the value for the given bin.
    ///
    /// Bounds checking is not performed, as the garbage collector never uses an
    /// out of bounds index.
    pub fn get(&self, index: usize) -> u32 {
        debug_assert!(
            index < self.values.len(),
            "index is {} but the length is {}",
            index,
            self.values.len()
        );

        self.values[index]
    }

    /// Removes all values from the histogram.
    pub fn reset(&mut self) {
        self.values.reset();
    }
}

/// A collection of histograms that Immix will use for determining when to move
/// objects.
pub struct Histograms {
    // The available space histogram for the blocks of this allocator.
    pub available: Histogram,

    /// The mark histogram for the blocks of this allocator.
    pub marked: Histogram,
}

unsafe impl Sync for Histograms {}

impl Histograms {
    pub fn new() -> Self {
        Self {
            available: Histogram::new(MAX_HOLES + 1),
            marked: Histogram::new(LINES_PER_BLOCK + 1),
        }
    }

    pub fn reset(&mut self) {
        self.available.reset();
        self.marked.reset();
    }
}
