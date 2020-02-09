use super::*;
use crate::runtime::cell::*;
use crate::util::mem::Address;
use crate::util::ptr::*;
use crate::util::tagged::*;
use intrusive_collections::{LinkedList, LinkedListLink};
use space::*;
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
            self.value.mark(true);
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
        let mut tmp_space = super::space::Space::new(space);
        std::mem::swap(&mut self.tmp_space, &mut tmp_space);
        self.process_grey(heap);

        while let Some(item) = self.black_items.pop_back() {
            item.value.mark(false);
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
            heap.needs_gc = GCType::Young;
        } else {
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

            if !value.value.is_marked() {
                if !self.is_in_current_space(&value) {
                    if !value.value.is_soft_marked() {
                        value.value.soft_mark(true);
                        value.value.mark(true);
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
                value.relocate(hvalue);
                value.value.get().trace(|ptr| {
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

    pub fn is_in_current_space(&self, value: &GCValue) -> bool {
        if self.gc_ty == GCType::Old {
            value.value.get().generation >= 5
        } else {
            value.value.get().generation < 5
        }
    }
}
