pub mod cms;
pub mod freelist;
pub mod freelist_alloc;
pub mod gc_pool;
pub mod incremental;
pub mod semispace;
use crate::runtime::cell::*;
use crate::runtime::config::*;
use crate::runtime::value::*;
use crate::util::arc::*;
pub mod space;
use crate::util::mem::{align_usize, page_size};
use space::*;
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash)]
pub enum GCType {
    None,
    Young,
    Old,
}

use structopt::StructOpt;

#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash, StructOpt)]
#[structopt(name = "GC Variant", help = "GC type to use for garbage collection")]
pub enum GCVariant {
    #[structopt(name = "semispace", help = "Semisapce generational GC")]
    GenerationalSemispace,
    #[structopt(name = "mark-compact", help = "Mark-Compact GC")]
    MarkCompact,
    #[structopt(name = "mark-sweep", help = "Mark&Sweep GC")]
    MarkAndSweep,
    #[structopt(name = "inc-mark-sweep", help = "Incremental Mark&Sweep GC")]
    IncrementalMarkCompact,
    IncrementalMarkSweep,
    GenIncMarkSweep,
}

impl std::str::FromStr for GCVariant {
    type Err = String;
    fn from_str(s: &str) -> Result<GCVariant, Self::Err> {
        let s = s.to_lowercase();
        let s_: &str = &s;
        Ok(match s_ {
            "semispace" => Self::GenerationalSemispace,
            "mark-compact" | "mark compact" => Self::MarkCompact,
            "mark-sweep" | "mark and sweep" | "mark&sweep" => Self::MarkAndSweep,
            "incremental mark-sweep" | "incremental-mark-sweep" => Self::IncrementalMarkSweep,
            "generational mark-sweep" => Self::GenIncMarkSweep,
            _ => return Err(format!("Unknown GC Type '{}'", s)),
        })
    }
}

pub fn initialize_process_heap(variant: GCVariant, config: &Config) -> Box<dyn HeapTrait> {
    match variant {
        GCVariant::GenerationalSemispace => Box::new(semispace::GenerationalCopyGC {
            heap: semispace::Heap::new(
                align_usize(config.young_size, page_size()),
                align_usize(config.old_size, page_size()),
            ),
            gc: semispace::GC::new(),
            threshold: config.young_threshold,
            mature_threshold: config.mature_threshold,
        }),
        GCVariant::IncrementalMarkSweep => Box::new(incremental::IncrementalCollector::new(
            false,
            align_usize(config.heap_size, page_size()),
        )),
        GCVariant::GenIncMarkSweep => Box::new(incremental::IncrementalCollector::new(
            true,
            align_usize(config.heap_size, page_size()),
        )),

        _ => unimplemented!(),
    }
}

/// Permanent heap.
///
/// Values that will not be collected and *must* be alive through entire program live should be allocated in perm heap.
pub struct PermanentHeap {
    pub space: Space,
    pub allocated: Vec<CellPointer>,
}

impl PermanentHeap {
    pub fn new(perm_size: usize) -> Self {
        Self {
            space: Space::new(perm_size),
            allocated: Vec::with_capacity(64),
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
        unsafe { cell.set_permanent() };
        self.allocated.push(cell);
        Value::from(cell)
    }
}

impl Drop for PermanentHeap {
    fn drop(&mut self) {
        while let Some(cell) = self.allocated.pop() {
            unsafe {
                std::ptr::drop_in_place(cell.raw.raw);
            }
        }
        self.space.clear();
    }
}

pub trait HeapTrait {
    /// Returns true if GC should be triggered.
    fn should_collect(&self) -> bool;
    /// Allocate CellPointer
    fn allocate(&mut self, tenure: GCType, cell: Cell) -> CellPointer;
    /// Copy object from one heap to another heap.
    fn copy_object(&mut self, object: Value) -> Value {
        if !object.is_cell() {
            return object;
        }

        let to_copy = object.as_cell();
        if to_copy.is_permanent() {
            return object;
        }
        let to_copy = to_copy.get();
        let value_copy = match &to_copy.value {
            CellValue::None => CellValue::None,
            CellValue::Duration(d) => CellValue::Duration(d.clone()),
            CellValue::File(_) => panic!("Cannot copy file"),
            CellValue::Number(x) => CellValue::Number(*x),
            CellValue::Bool(x) => CellValue::Bool(*x),
            CellValue::String(x) => CellValue::String(x.clone()),
            CellValue::Array(values) => {
                let new_values = values
                    .iter()
                    .map(|value| self.copy_object(*value))
                    .collect();
                CellValue::Array(Box::new(new_values))
            }
            CellValue::Function(function) => {
                let name = function.name.clone();
                let argc = function.argc.clone();
                let module = function.module.clone();
                let upvalues = function
                    .upvalues
                    .iter()
                    .map(|x| self.copy_object(*x))
                    .collect();
                let native = function.native;
                let code = function.code.clone();
                CellValue::Function(Arc::new(Function {
                    name,
                    argc,
                    module,
                    upvalues,
                    native,
                    code,
                }))
            }
            CellValue::ByteArray(array) => CellValue::ByteArray(array.clone()),
            CellValue::Module(module) => CellValue::Module(module.clone()),
            CellValue::Process(proc) => CellValue::Process(proc.clone()),
        };
        let mut copy = if let Some(proto_ptr) = to_copy.prototype {
            let proto_copy = self.copy_object(Value::from(proto_ptr));
            Cell::with_prototype(value_copy, proto_copy.as_cell())
        } else {
            Cell::new(value_copy)
        };
        if let Some(map) = to_copy.attributes_map() {
            let mut map_copy = AttributesMap::with_capacity(map.len());
            for (key, val) in map.iter() {
                let key_copy = key.clone();
                let val = self.copy_object(*val);
                map_copy.insert(key_copy, val);
            }

            copy.set_attributes_map(map_copy);
        }

        Value::from(self.allocate(GCType::Young, copy))
    }
    /// Collect garbage.
    fn collect_garbage(&mut self, proc: &Arc<crate::runtime::process::Process>);
    /// Minor GC cycle.
    ///
    /// If incremental algorithm is used this should trigger incremental mark&sweep.
    fn minor_collect(&mut self, proc: &Arc<crate::runtime::process::Process>) {
        self.collect_garbage(proc);
    }
    /// Major GC cycle.
    fn major_collect(&mut self, proc: &Arc<crate::runtime::process::Process>) {
        self.collect_garbage(proc);
    }
    /// Clear memory.
    fn clear(&mut self) {}
    /// "Schedule" pointer into GC mark list.
    fn schedule(&mut self, _: *mut CellPointer);
    fn write_barrier(&mut self, _: CellPointer) {}
    /// Colours 'parent' as gray object if child is white and parent is black objects.
    fn field_write_barrier(&mut self, _: CellPointer, _: Value) {}
    /// Read barrier is used when background GC is enabled.
    fn read_barrier(&mut self, _: *const CellPointer) {}
    /// Remember object so this object will not be collected even if it's not reachable.
    fn remember(&mut self, _: CellPointer) {}
    /// Unremember object so this object may be collected.
    fn unremember(&mut self, _: CellPointer) {}

    fn trace_process(&mut self, proc: &Arc<crate::runtime::process::Process>);
    fn set_proc(&mut self, _proc: Arc<crate::runtime::process::Process>) {}
}
