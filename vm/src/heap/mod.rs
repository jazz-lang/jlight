pub mod copy;
pub mod gc_pool;
pub mod space;
use crate::runtime::cell::*;
use crate::runtime::config::*;
use crate::runtime::value::*;
use crate::util::arc::*;
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash)]
pub enum GCType {
    None,
    Young,
    Old,
}

#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash)]
pub enum GCVariant {
    GenerationalSemispace,
    MarkCompact,
    MarkAndSweep,
    IncrementalMarkCompact,
}

pub fn initialize_process_heap(variant: GCVariant, config: &Config) -> Box<dyn HeapTrait> {
    match variant {
        GCVariant::GenerationalSemispace => Box::new(copy::GenerationalCopyGC {
            heap: copy::Heap::new(config.young_size, config.old_size),
            gc: copy::GC::new(),
            threshold: config.gc_threshold,
        }),

        _ => unimplemented!(),
    }
}

/// Permanent heap.
///
/// Values that will not be collected and *must* be alive through entire program live should be allocated in perm heap.
pub struct PermanentHeap {
    pub space: space::Space,
}

impl PermanentHeap {
    pub fn new(perm_size: usize) -> Self {
        Self {
            space: space::Space::new(perm_size),
        }
    }
    pub fn allocate_empty(&mut self) -> Value {
        self.allocate(Cell::new(CellValue::None))
    }
    pub fn allocate(&mut self, cell: Cell) -> Value {
        let pointer = self
            .space
            .allocate(std::mem::size_of::<Cell>(), &mut false)
            .to_mut_ptr::<Cell>();
        unsafe {
            pointer.write(cell);
        }
        let mut cell = CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(pointer),
        };
        cell.set_permanent();
        Value::from(cell)
    }
}

impl Drop for PermanentHeap {
    fn drop(&mut self) {
        self.space.clear();
    }
}

pub trait HeapTrait {
    fn should_collect(&self) -> bool;
    fn allocate(&mut self, tenure: GCType, cell: Cell) -> CellPointer;
    fn copy_object(&mut self, value: Value) -> Value;
    fn collect_garbage(&mut self);
    fn clear(&mut self) {}
    fn schedule(&mut self, _: *mut CellPointer);
    fn write_barrier(&mut self, _: CellPointer, _: Value) {}
    fn read_barrier(&mut self, _: Value) {}
}
