pub mod gc;
pub mod space;
use crate::runtime::cell::*;
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug, Hash)]
pub enum GCType {
    None,
    Young,
    Old,
}

pub struct Heap {
    pub new_space: space::Space,
    pub old_space: space::Space,
    pub needs_gc: GCType,
}

impl Heap {
    pub fn new(page_size: usize) -> Self {
        Self {
            new_space: space::Space::new(page_size),
            old_space: space::Space::new(page_size),
            needs_gc: GCType::None,
        }
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
        CellPointer {
            raw: crate::util::tagged::TaggedPointer::new(result),
        }
    }
}
