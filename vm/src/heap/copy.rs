use super::space::*;
use super::*;
use crate::runtime::cell::*;
use crate::runtime::value::*;
use crate::util::arc::*;
use crate::util::mem::*;
use std::boxed::Box;
pub struct Heap {
    pub new_space: space::Space,
    pub old_space: space::Space,
    pub needs_gc: GCType,
    /// We keep track of all allocated objects so we can properly deallocate them at GC cycle or when this heap is destroed.
    pub allocated: Vec<CellPointer>,
}

impl Heap {
    pub fn new(young_page_size: usize, old_page_size: usize) -> Self {
        Self {
            new_space: space::Space::new(young_page_size),
            old_space: space::Space::new(old_page_size),
            needs_gc: GCType::None,
            allocated: Vec::new(),
        }
    }

    pub fn copy_object(&mut self, object: Value) -> Value {
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
                CellValue::Array(new_values)
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

    pub fn allocate(&mut self, tenure: GCType, cell: Cell) -> CellPointer {
        assert_ne!(tenure, GCType::None);
        let space = if tenure == GCType::Old {
            &mut self.old_space
        } else {
            &mut self.new_space
        };
        let mut needs_gc = false;
        let result = space
            .allocate(std::mem::size_of::<Cell>(), &mut needs_gc)
            .to_mut_ptr::<Cell>();
        unsafe {
            result.write(cell);
        }
        self.needs_gc = if needs_gc { tenure } else { GCType::None };
        self.allocated.push(CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(result),
        });
        CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(result),
        }
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        while let Some(cell) = self.allocated.pop() {
            unsafe {
                if cell.raw.is_null() == false {
                    std::ptr::drop_in_place(cell.raw.raw);
                }
            }
        }
        self.new_space.clear();
        self.old_space.clear();
    }
}

use crate::util::ptr::*;
use crate::util::tagged::*;
use intrusive_collections::{LinkedList, LinkedListLink};
pub struct GCValue {
    pub slot: *mut CellPointer,
    pub value: CellPointer,
    link: LinkedListLink,
}

impl GCValue {
    pub fn relocate(&mut self, address: CellPointer) {
        if self.slot.is_null() == false {
            unsafe {
                self.slot.write(address);
            }
        }
        if !self.value.is_marked() {
            self.value.set_color(CELL_BLACK);
            self.value.get_mut().forward = Address::from_ptr(address.raw.raw);
        }
    }
}

intrusive_adapter!(
    GCValueAdapter =  Box<GCValue> : GCValue {link: LinkedListLink}
);

pub struct GC {
    grey_items: LinkedList<GCValueAdapter>,
    black_items: LinkedList<GCValueAdapter>,
    tmp_space: space::Space,
    gc_ty: GCType,
}

impl GC {
    pub fn new() -> Self {
        Self {
            tmp_space: space::Space::empty(),
            grey_items: LinkedList::new(GCValueAdapter::new()),
            black_items: LinkedList::new(GCValueAdapter::new()),
            gc_ty: GCType::None,
        }
    }

    pub fn collect_garbage(&mut self, heap: &mut Heap) {
        if heap.needs_gc == GCType::None {
            heap.needs_gc = GCType::Young;
        }
        self.gc_ty = heap.needs_gc;
        let space = if self.gc_ty == GCType::Young {
            heap.new_space.page_size
        } else {
            heap.old_space.page_size
        };
        log::trace!(
            "Begin {:?} space collection (current worker is '{}')",
            self.gc_ty,
            std::thread::current().name().unwrap()
        );
        let mut tmp_space = super::space::Space::new(space);
        std::mem::swap(&mut self.tmp_space, &mut tmp_space);
        self.process_grey(heap);
        heap.allocated.retain(|cell| {
            let in_current_space = self.is_in_current_space(cell);
            debug_assert!(cell.get_color() != CELL_GREY);
            if in_current_space {
                if cell.get_color() == CELL_BLACK || cell.is_soft_marked() {
                    if cell.get_color() != CELL_WHITE_A {
                        cell.set_color(CELL_WHITE_A);
                    }
                    return true;
                } else {
                    log::trace!("Finalize {:p}", cell.raw.raw);
                    unsafe {
                        std::ptr::drop_in_place(cell.raw.raw);
                    }
                    false
                }
            } else {
                true
            }
        });
        while let Some(item) = self.black_items.pop_back() {
            item.value.set_color(CELL_WHITE_A);
            item.value.soft_mark(false);
        }
        std::mem::swap(&mut self.tmp_space, &mut tmp_space);
        let space = if self.gc_ty == GCType::Young {
            &mut heap.new_space
        } else {
            &mut heap.old_space
        };
        space.swap(&mut tmp_space);
        if self.gc_ty != GCType::Young || heap.needs_gc == GCType::Young {
            heap.needs_gc = GCType::None;
            log::trace!("Collection finished");
        } else {
            log::trace!("Young space collected, collecting Old space");
            // Do GC for old space.
            self.collect_garbage(heap);
        }
    }

    pub fn schedule(&mut self, ptr: *mut CellPointer) {
        self.grey_items.push_back(Box::new(GCValue {
            link: LinkedListLink::new(),
            slot: ptr as *mut CellPointer,
            value: unsafe { *ptr },
        }))
    }

    pub fn process_grey(&mut self, heap: &mut Heap) {
        while self.grey_items.is_empty() != true {
            let mut value = self.grey_items.pop_back().unwrap();
            if value.value.raw.is_null() {
                continue;
            }
            log::trace!(
                "Process {:p} (color is {})",
                value.value.raw.raw,
                match value.value.get_color() {
                    CELL_WHITE_A => "white",
                    CELL_BLACK => "black",
                    CELL_GREY => "grey",
                    _ => unreachable!(),
                }
            );
            if value.value.get_color() == CELL_WHITE_A {
                if !self.is_in_current_space(&value.value) {
                    log::trace!(
                        "{:p} is not in {:?} space (generation: {})",
                        value.value.raw.raw,
                        self.gc_ty,
                        value.value.get().generation
                    );
                    if !value.value.is_soft_marked() {
                        value.value.soft_mark(true);
                        value.value.set_color(CELL_BLACK);
                        value.value.get().trace(|ptr| {
                            self.grey_items.push_back(Box::new(GCValue {
                                link: LinkedListLink::new(),
                                slot: ptr as *mut CellPointer,
                                value: unsafe { *ptr },
                            }))
                        });
                        self.black_items.push_back(value);
                    }
                    continue;
                }
                let hvalue;
                if self.gc_ty == GCType::Young {
                    hvalue =
                        value
                            .value
                            .copy_to(&mut heap.old_space, &mut self.tmp_space, &mut false);
                } else {
                    hvalue =
                        value
                            .value
                            .copy_to(&mut self.tmp_space, &mut heap.new_space, &mut false);
                }
                log::trace!("Copy {:p}->{:p}", value.value.raw.raw, hvalue.raw.raw);
                value.relocate(hvalue);
                value.value.get().trace(|ptr| {
                    unsafe {
                        (*ptr).set_color(CELL_GREY);
                    }
                    self.grey_items.push_back(Box::new(GCValue {
                        link: LinkedListLink::new(),
                        slot: ptr as *mut CellPointer,
                        value: unsafe { *ptr },
                    }))
                })
            } else {
                let fwd = value.value.get().forward.to_mut_ptr::<Cell>();
                value.relocate(CellPointer {
                    raw: TaggedPointer::new(fwd),
                });
            }
        }
    }

    pub fn is_in_current_space(&self, value: &CellPointer) -> bool {
        if value.is_permanent() {
            log::trace!("Found permanent object {:p}, will skip it", value.raw.raw);
            return false; // we don't want to move permanent objects
        }
        if self.gc_ty == GCType::Old {
            value.get().generation >= 5
        } else {
            value.get().generation < 5
        }
    }
}

/// Semi-space generational GC.
pub struct GenerationalCopyGC {
    pub heap: Heap,
    pub gc: GC,
    pub threshold: usize,
}

// after collection we want the the ratio of used/total to be no
// greater than this (the threshold grows exponentially, to avoid
// quadratic behavior when the heap is growing linearly with the
// number of `new` calls):
const USED_SPACE_RATIO: f64 = 0.7;

impl HeapTrait for GenerationalCopyGC {
    fn should_collect(&self) -> bool {
        self.heap.needs_gc == GCType::Young || self.heap.new_space.size >= self.threshold
    }

    fn allocate(&mut self, tenure: GCType, cell: Cell) -> CellPointer {
        self.heap.allocate(tenure, cell)
    }

    fn collect_garbage(&mut self) {
        if self.heap.needs_gc != GCType::Young {
            self.heap.needs_gc = GCType::Young;
        }
        self.gc.collect_garbage(&mut self.heap);
        if (self.threshold as f64) < self.heap.new_space.allocated_size as f64 * USED_SPACE_RATIO {
            self.threshold =
                (self.heap.new_space.allocated_size as f64 / USED_SPACE_RATIO) as usize;
        }
    }

    fn copy_object(&mut self, value: Value) -> Value {
        self.heap.copy_object(value)
    }

    fn clear(&mut self) {
        self.heap.old_space.clear();
        self.heap.new_space.clear();
    }

    fn schedule(&mut self, ptr: *mut CellPointer) {
        self.gc.grey_items.push_back(Box::new(GCValue {
            value: unsafe { *ptr },
            slot: ptr,
            link: LinkedListLink::new(),
        }));
    }
}
