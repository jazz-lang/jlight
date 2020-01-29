use crate::heap;
use crate::heap::map::*;
use crate::object_value::ObjectValue;
use crate::process::RcProcess;
use crate::state::*;
use crate::tagged_pointer::TaggedPointer;
use ahash::AHashMap;
use heap::bucket::*;
use heap::generation_config::*;
use heap::local_allocator::YOUNG_MAX_AGE;
use heap::*;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::f32;
use std::f64;
use std::fs;
use std::hash::{Hash, Hasher as HasherTrait};
use std::i16;
use std::i32;
use std::i64;
use std::i8;
use std::ops::Drop;
use std::ptr;
use std::u16;
use std::u32;
use std::u64;
use std::u8;
use std::usize;

pub type AttributesMap = AHashMap<ObjectPointer, ObjectPointer>;

macro_rules! push_collection {
    ($map:expr, $what:ident, $vec:expr) => {{
        $vec.reserve($map.len());

        for thing in $map.$what() {
            $vec.push(*thing);
        }
    }};
}

/// Defines a method for getting the value of an object as a given type.
macro_rules! def_value_getter {
    ($name: ident, $getter: ident, $as_type: ident, $ok_type: ty) => (
        pub fn $name(&self) -> Result<$ok_type, String> {
            if self.is_tagged_integer() {
                Err(format!("ObjectPointer::{}() called on a tagged integer",
                            stringify!($as_type)))
            } else {
                self.$getter().value.$as_type()
            }
        }
    )
}

macro_rules! def_integer_value_getter {
    ($name: ident, $kind: ident, $error_name: expr) => (
        pub fn $name(&self) -> Result<$kind, String> {
            let int_val = self.integer_value()?;

            if int_val < i64::from($kind::MIN) || int_val > i64::from($kind::MAX) {
                Err(format!(
                    "{} can not be converted to a {}",
                    int_val,
                    $error_name
                ))
            } else {
                Ok(int_val as $kind)
            }
        }
    )
}
/// The minimum integer value that can be stored as a tagged integer.
pub const MIN_INTEGER: i64 = i64::MIN >> 1;

/// The maximum integer value that can be stored as a tagged integer.
pub const MAX_INTEGER: i64 = i64::MAX >> 1;

pub type RawObjectPointer = *mut Object;
unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// A pointer to a object pointer. This wrapper is necessary to allow sharing
/// *const ObjectPointer pointers between threads.
#[derive(Clone)]
pub struct ObjectPointerPointer {
    pub raw: *const ObjectPointer,
}

unsafe impl Send for ObjectPointerPointer {}
unsafe impl Sync for ObjectPointerPointer {}

/// The bit to set for tagged integers.
pub const INTEGER_BIT: usize = 0;

/// The status of an object.
#[derive(Eq, PartialEq, Debug)]
pub enum ObjectStatus {
    /// This object is OK and no action has to be taken by a collector.
    OK,

    /// This object has been forwarded and all forwarding pointers must be
    /// resolved.
    Resolve,

    /// This object is ready to be promoted to the mature generation.
    Promote,

    /// This object should be evacuated from its block.
    Evacuate,

    /// This object is in the process of being moved.
    PendingMove,
}

/// The bit to set for objects that are being forwarded.
pub const PENDING_FORWARD_BIT: usize = 0;

/// The bit to set for objects that have been forwarded.
pub const FORWARDED_BIT: usize = 1;

/// The bit to set for objects stored in a remembered set.
pub const REMEMBERED_BIT: usize = 2;

/// The mask to apply when installing a forwarding pointer.
pub const FORWARDING_MASK: usize = 0x3;

pub struct Object {
    pub prototype: ObjectPointer,
    pub attributes: TaggedPointer<AttributesMap>,
    pub value: ObjectValue,
}

impl Object {
    pub fn write_to(self, raw_pointer: RawObjectPointer) -> ObjectPointer {
        let pointer = ObjectPointer::new(raw_pointer);

        // Finalize the existing object, if needed. This must be done before we
        // allocate the new object, otherwise we will leak memory.
        pointer.finalize();

        // Write the new data to the pointer.
        unsafe {
            ptr::write(raw_pointer, self);
        }

        pointer
    }
    /// Returns a new object with the given value.
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: ObjectPointer::null(),
            attributes: TaggedPointer::null(),
            value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(value: ObjectValue, prototype: ObjectPointer) -> Object {
        Object {
            prototype,
            attributes: TaggedPointer::null(),
            value,
        }
    }

    /// Sets the prototype of this object.
    pub fn set_prototype(&mut self, prototype: ObjectPointer) {
        self.prototype = prototype;
    }

    /// Returns the prototype of this object.
    pub fn prototype(&self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            Some(self.prototype)
        }
    }

    /// Returns and removes the prototype of this object.
    pub fn take_prototype(&mut self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            let proto = self.prototype;

            self.prototype = ObjectPointer::null();

            Some(proto)
        }
    }

    /// Removes an attribute and returns it.
    pub fn remove_attribute(&mut self, name: ObjectPointer) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map_mut() {
            map.remove(&name)
        } else {
            None
        }
    }

    /// Returns all the attributes available to this object.
    pub fn attributes(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, values, attributes);
        }

        attributes
    }

    /// Returns all the attribute names available to this object.
    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, keys, attributes);
        }

        attributes
    }

    /// Looks up an attribute in either the current object or a parent object.
    pub fn lookup_attribute(&self, name: ObjectPointer) -> Option<ObjectPointer> {
        let got = self.lookup_attribute_in_self(name);

        if got.is_some() {
            return got;
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype().is_some() {
            let mut opt_parent = self.prototype();

            while let Some(parent_ptr) = opt_parent {
                let parent = parent_ptr.get();
                let got = parent.lookup_attribute_in_self(name);

                if got.is_some() {
                    return got;
                }

                opt_parent = parent.prototype();
            }
        }

        None
    }
    /// Allocates an attribute map if needed.
    fn allocate_attributes_map(&mut self) {
        if !self.has_attributes() {
            self.set_attributes_map(AttributesMap::default());
        }
    }
    /// Adds a new attribute to the current object.
    pub fn add_attribute(&mut self, name: ObjectPointer, object: ObjectPointer) {
        self.allocate_attributes_map();

        self.attributes_map_mut().unwrap().insert(name, object);
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(&self, name: ObjectPointer) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map() {
            map.get(&name).cloned()
        } else {
            None
        }
    }

    /// Returns an immutable reference to the attributes.
    pub fn attributes_map(&self) -> Option<&AttributesMap> {
        self.attributes.as_ref()
    }

    pub fn attributes_map_mut(&self) -> Option<&mut AttributesMap> {
        self.attributes.as_mut()
    }

    pub fn set_attributes_map(&mut self, attrs: AttributesMap) {
        self.attributes = TaggedPointer::new(Box::into_raw(Box::new(attrs)));
    }

    /// Returns true if this object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_attributes()
    }
    /// Returns true if an attributes map has been allocated.
    pub fn has_attributes(&self) -> bool {
        if self.is_forwarded() {
            return false;
        }

        !self.attributes.untagged().is_null()
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj = Object::with_prototype(self.value.take(), self.prototype);

        // When taking over the attributes we want to automatically inherit the
        // "remembered" bit, but not the forwarding bits.
        let attrs = (self.attributes.raw as usize & !FORWARDING_MASK) as *mut AttributesMap;

        new_obj.attributes = TaggedPointer::new(attrs);

        // When the object is being forwarded we don't want to lose this status
        // by just setting the attributes to NULL. Doing so could result in
        // another collector thread to try and move the same object.
        self.attributes = TaggedPointer::with_bit(0x0 as _, PENDING_FORWARD_BIT);

        new_obj
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        if !self.prototype.is_null() {
            callback(self.prototype.pointer());
        }

        if let Some(map) = self.attributes_map() {
            for (_, pointer) in map.iter() {
                callback(pointer.pointer());
            }
        }

        match self.value {
            ObjectValue::Array(ref array) => {
                array.iter().for_each(|x| callback(x.pointer()));
            }
            ObjectValue::Function(ref function) => {
                function.upvalues.iter().for_each(|x| callback(x.pointer()));
            }
            _ => (),
        }
    }

    /// Tries to mark this object as pending a forward.
    ///
    /// This method returns true if forwarding is necessary, false otherwise.
    pub fn mark_for_forward(&mut self) -> bool {
        // This _must_ be a reference, otherwise we'll be operating on a _copy_
        // of the pointer, since TaggedPointer is a Copy type.
        let current = &mut self.attributes;
        let current_raw = current.raw;

        if current.atomic_bit_is_set(PENDING_FORWARD_BIT) {
            // Another thread is in the process of forwarding this object, or
            // just finished forwarding it (since forward_to() sets both bits).
            return false;
        }

        let desired = TaggedPointer::with_bit(current_raw, PENDING_FORWARD_BIT).raw;

        current.compare_and_swap(current_raw, desired)
    }

    pub fn drop_attributes(&mut self) {
        if !self.has_attributes() {
            return;
        }

        drop(unsafe { Box::from_raw(self.attributes.untagged()) });

        self.attributes = TaggedPointer::null();
    }
    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        // We use a mask that sets the lower 2 bits, instead of only setting
        // one. This removes the need for checking both bits when determining if
        // forwarding is necessary.
        let new_attrs = (pointer.raw.raw as usize | FORWARDING_MASK) as *mut AttributesMap;

        self.attributes.atomic_store(new_attrs);
    }

    /// Marks this object as being remembered.
    ///
    /// This does not use atomic operations and thus should not be called
    /// concurrently for the same pointer.
    pub fn mark_as_remembered(&mut self) {
        self.attributes.set_bit(REMEMBERED_BIT);
    }

    /// Returns true if this object has been remembered in a remembered set.
    pub fn is_remembered(&self) -> bool {
        self.attributes.atomic_bit_is_set(REMEMBERED_BIT)
    }

    /// Returns true if this object is forwarded.
    pub fn is_forwarded(&self) -> bool {
        self.attributes.atomic_bit_is_set(FORWARDED_BIT)
    }
}
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    pub raw: TaggedPointer<Object>,
}
unsafe impl Sync for Object {}
unsafe impl Send for Object {}

impl ObjectPointer {
    /// Returns true if the current pointer points to a permanent object.
    pub fn is_permanent(&self) -> bool {
        self.is_tagged_integer() || self.block().bucket().unwrap().age == PERMANENT
    }

    /// Returns true if the current pointer points to a mature object.
    pub fn is_mature(&self) -> bool {
        !self.is_tagged_integer() && self.block().bucket().unwrap().age == MATURE
    }

    /// Returns true if the current pointer points to a mailbox object.
    pub fn is_mailbox(&self) -> bool {
        !self.is_tagged_integer() && self.block().bucket().unwrap().age == MAILBOX
    }

    /// Returns true if the current pointer points to a young object.
    pub fn is_young(&self) -> bool {
        !self.is_tagged_integer() && self.block().bucket().unwrap().age <= YOUNG_MAX_AGE
    }

    pub fn string_value(&self) -> Result<&String, String> {
        if self.is_tagged_integer() {
            return Err(format!(
                "ObjectPointer::strign_value() called on a tagged integer"
            ));
        } else {
            self.get().value.as_string()
        }
    }

    /// Marks the current object and its line.
    ///
    /// As this method is called often during collection, this method refers to
    /// `self.raw` only once and re-uses the pointer. This ensures there are no
    /// race conditions when determining the object/line indexes, and reduces
    /// the overhead of having to call `self.raw.untagged()` multiple times.
    pub fn mark(&self) {
        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();

        let object_index = block.object_index_of_pointer(pointer);
        let line_index = block.line_index_of_pointer(pointer);

        block.marked_objects_bytemap.set(object_index);
        block.used_lines_bytemap.set(line_index);
    }

    /// Unmarks the current object.
    ///
    /// The line mark state is not changed.
    pub fn unmark(&self) {
        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();

        let object_index = block.object_index_of_pointer(pointer);

        block.marked_objects_bytemap.unset(object_index);
    }

    /// Returns true if the current object is marked.
    ///
    /// This method *must not* use any methods that also call
    /// `self.raw.untagged()` as doing so will lead to race conditions producing
    /// incorrect object/line indexes. This can happen when one tried checks if
    /// an object is marked while another thread is updating the pointer's
    /// address (e.g. after evacuating the underlying object).
    pub fn is_marked(&self) -> bool {
        if self.is_tagged_integer() {
            return true;
        }

        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();
        let index = block.object_index_of_pointer(pointer);

        block.marked_objects_bytemap.is_set(index)
    }

    /// Marks the object this pointer points to as being remembered in a
    /// remembered set.
    pub fn mark_as_remembered(&self) {
        self.get_mut().mark_as_remembered();
    }

    /// Returns `true` if the object this pointer points to has been remembered
    /// in a remembered set.
    pub fn is_remembered(&self) -> bool {
        self.get().is_remembered()
    }

    /// Returns a mutable reference to the block this pointer belongs to.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn block_mut(&self) -> &mut block::Block {
        block_header_of(self.raw.untagged()).block_mut()
    }

    /// Returns an immutable reference to the block this pointer belongs to.
    #[inline(always)]
    pub fn block(&self) -> &block::Block {
        block_header_of(self.raw.untagged()).block()
    }

    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::new(pointer),
        }
    }

    /// Creates a new tagged integer.
    pub fn integer(value: i64) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::with_bit((value << 1) as RawObjectPointer, INTEGER_BIT),
        }
    }

    pub fn byte(value: u8) -> ObjectPointer {
        Self::integer(i64::from(value))
    }

    /// Returns `true` if the given unsigned integer is too large for a tagged
    /// pointer.
    pub fn unsigned_integer_too_large(value: u64) -> bool {
        value > MAX_INTEGER as u64
    }

    /// Returns `true` if the given unsigned integer should be allocated as a
    /// big integer.
    pub fn unsigned_integer_as_big_integer(value: u64) -> bool {
        value > i64::MAX as u64
    }

    /// Returns `true` if the given value is too large for a tagged pointer.
    pub fn integer_too_large(value: i64) -> bool {
        value < MIN_INTEGER || value > MAX_INTEGER
    }

    /// Creates a new null pointer.
    pub const fn null() -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::null(),
        }
    }

    pub fn is_immutable(&self) -> bool {
        self.is_tagged_integer() || self.get().value.is_immutable()
    }

    pub fn integer_value(&self) -> Result<i64, String> {
        if self.is_tagged_integer() {
            Ok(self.raw.raw as i64 >> 1)
        } else if let Ok(num) = self.get().value.as_number() {
            Ok(num as i64)
        } else {
            Err("ObjectPointer::integer_value() called on a non integer object".to_string())
        }
    }

    pub fn number_value(&self) -> Result<f64, String> {
        if self.is_tagged_integer() {
            Ok(f64::from_bits((self.raw.raw as i64 >> 1) as u64))
        } else if let Ok(num) = self.get().value.as_number() {
            Ok(num)
        } else {
            Err("ObjectPointer::number_value() called on a non number object".to_string())
        }
    }

    /// Returns an immutable reference to the Object.
    #[inline(always)]
    pub fn get(&self) -> &Object {
        self.raw
            .as_ref()
            .expect("ObjectPointer::get() called on a NULL pointer")
    }

    /// Returns a mutable reference to the Object.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn get_mut(&self) -> &mut Object {
        self.raw
            .as_mut()
            .expect("ObjectPointer::get_mut() called on a NULL pointer")
    }

    /// Returns true if the current pointer is a null pointer.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.raw.raw as usize == 0x0
    }

    /// Returns true if the object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        !self.is_tagged_integer() && self.get().is_finalizable()
    }

    /// Finalizes the underlying object, if needed.
    pub fn finalize(&self) {
        if !self.is_finalizable() {
            return;
        }

        unsafe {
            ptr::drop_in_place(self.raw.raw);

            // We zero out the memory so future finalize() calls for the same
            // object (before other allocations take place) don't try to free
            // the memory again.
            ptr::write_bytes(self.raw.raw, 0, 1);
        }
    }

    /// Returns a pointer to this pointer.
    pub fn pointer(&self) -> ObjectPointerPointer {
        ObjectPointerPointer::new(self)
    }
    pub fn is_tagged_integer(&self) -> bool {
        self.raw.bit_is_set(INTEGER_BIT)
    }

    pub fn add_attribute(&self, process: &RcProcess, name: ObjectPointer, attr: ObjectPointer) {
        self.get_mut().add_attribute(name, attr);
        process.write_barrier(*self, attr);
    }
    pub fn lookup_attribute(&self, state: &RcState, name: ObjectPointer) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.number_prototype.get().lookup_attribute(name)
        } else {
            self.get().lookup_attribute(name)
        }
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        state: &RcState,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.number_prototype.get().lookup_attribute_in_self(name)
        } else {
            self.get().lookup_attribute_in_self(name)
        }
    }
    pub fn is_string(&self) -> bool {
        if self.is_tagged_integer() {
            false
        } else {
            self.get().value.is_string()
        }
    }

    pub fn is_number(&self) -> bool {
        if self.is_tagged_integer() {
            return true;
        } else {
            self.get().value.is_number()
        }
    }

    pub fn is_bigint(&self) -> bool {
        if self.is_tagged_integer() {
            return false;
        } else {
            self.get().value.is_bigint()
        }
    }

    pub fn attributes(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attributes()
        }
    }

    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attribute_names()
        }
    }

    pub fn set_prototype(&self, proto: ObjectPointer) {
        self.get_mut().set_prototype(proto);
    }

    pub fn prototype(&self, state: &RcState) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            Some(state.number_prototype)
        } else {
            self.get().prototype()
        }
    }

    pub fn is_kind_of(&self, state: &RcState, other: ObjectPointer) -> bool {
        let mut prototype = self.prototype(state);

        while let Some(proto) = prototype {
            if proto == other {
                return true;
            }

            prototype = proto.prototype(state);
        }

        false
    }
}

impl ObjectPointerPointer {
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::trivially_copy_pass_by_ref))]
    pub fn new(pointer: &ObjectPointer) -> ObjectPointerPointer {
        ObjectPointerPointer {
            raw: pointer as *const ObjectPointer,
        }
    }

    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    pub fn get_mut(&self) -> &mut ObjectPointer {
        unsafe { &mut *(self.raw as *mut ObjectPointer) }
    }

    #[inline(always)]
    pub fn get(&self) -> &ObjectPointer {
        unsafe { &*self.raw }
    }
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointer {}

impl Hash for ObjectPointer {
    fn hash<H: HasherTrait>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl PartialEq for ObjectPointerPointer {
    fn eq(&self, other: &ObjectPointerPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointerPointer {}

/// Returns the BlockHeader of the given pointer.
fn block_header_of<'a>(pointer: RawObjectPointer) -> &'a mut block::BlockHeader {
    let addr = (pointer as isize & block::OBJECT_BYTEMAP_MASK) as usize;

    unsafe {
        let ptr = addr as *mut block::BlockHeader;

        &mut *ptr
    }
}
