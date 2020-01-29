use super::bucket::*;
use super::copy_object::*;
use super::global_allocator::RcGlobalAllocator;
use crate::{object::*, object_value::*};
use std::ops::Drop;
pub struct PermanentAllocator {
    global_allocator: RcGlobalAllocator,

    /// The bucket to allocate objects into.
    bucket: Bucket,
}

impl PermanentAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        PermanentAllocator {
            global_allocator,
            bucket: Bucket::with_age(PERMANENT),
        }
    }

    pub fn allocate_with_prototype(
        &mut self,
        value: ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        self.allocate(Object::with_prototype(value, proto))
    }

    pub fn allocate_without_prototype(&mut self, value: ObjectValue) -> ObjectPointer {
        self.allocate(Object::new(value))
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(ObjectValue::None)
    }

    fn allocate(&mut self, object: Object) -> ObjectPointer {
        let (_, pointer) = self.bucket.allocate(&self.global_allocator, object);

        pointer.mark();
        pointer
    }
}

impl CopyObject for PermanentAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}

impl Drop for PermanentAllocator {
    fn drop(&mut self) {
        for block in self.bucket.blocks.drain() {
            // Dropping the block also finalises it right away.
            drop(block);
        }
    }
}
