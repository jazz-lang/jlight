use super::module::Module;
use crate::runtime::state::{RcState, State};
use crate::util::arc::Arc;
use crate::util::tagged_pointer::*;
use ahash::AHashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
pub enum ObjectValue {
    None,
    Number(f64),
    Bool(bool),
    String(Arc<String>),
    File(fs::File),
    Array(Vec<ObjectPointer>),
    ByteArray(Vec<u8>),
    Function(Function),
    Module(Arc<Module>),
    Thread(Option<std::thread::JoinHandle<ObjectPointer>>),
}

pub struct Object {
    pub prototype: ObjectPointer,
    pub attributes: TaggedPointer<AHashMap<Arc<String>, ObjectPointer>>,
    pub value: ObjectValue,
    pub marked: AtomicBool,
}

#[derive(Clone, Copy)]
pub struct ObjectPointer {
    pub raw: TaggedPointer<Object>,
}
pub struct ObjectPointerPointer {
    pub raw: *const ObjectPointer,
}

impl ObjectValue {
    pub fn take(&mut self) -> ObjectValue {
        std::mem::replace(self, ObjectValue::None)
    }

    pub fn should_deallocate_native(&self) -> bool {
        match *self {
            ObjectValue::None => false,
            _ => true,
        }
    }
}

pub type AttributesMap = AHashMap<Arc<String>, ObjectPointer>;

macro_rules! push_collection {
    ($map:expr, $what:ident, $vec:expr) => {{
        $vec.reserve($map.len());

        for thing in $map.$what() {
            $vec.push(thing.clone());
        }
    }};
}

pub type NativeFn = extern "C" fn(
    &Runtime,
    ObjectPointer,
    &[ObjectPointer],
) -> Result<ObjectPointer, ObjectPointer>;

use super::Runtime;
use crate::util::ptr::Ptr;
pub struct Function {
    /// function name
    pub name: Arc<String>,
    /// captured values from parent scope
    pub upvalues: Vec<ObjectPointer>,
    pub argc: i32,
    pub code: Ptr<Vec<crate::bytecode::block::BasicBlock>>,
    /// Native function pointer
    pub native: Option<NativeFn>,
    pub module: Arc<Module>,
    pub hotness: usize,
}

/// The bit to set for tagged integers.
pub const INTEGER_BIT: usize = 0;

unsafe impl Sync for ObjectPointer {}
unsafe impl Send for ObjectPointer {}

impl ObjectPointer {
    pub fn is_false(&self, state: &RcState) -> bool {
        if self.is_null() {
            return true;
        }
        if self.is_tagged_number() {
            self.number_value().unwrap() == 0.0
        } else {
            match self.get().value {
                ObjectValue::Bool(true) => false,
                ObjectValue::Bool(false) => true,
                ObjectValue::None => true,
                _ => *self == state.nil_prototype,
            }
        }
    }

    pub fn pointer(&self) -> ObjectPointerPointer {
        ObjectPointerPointer {
            raw: self as *const Self,
        }
    }

    pub fn is_marked(&self) -> bool {
        if self.is_tagged_number() {
            true
        } else {
            self.get().marked.load(Ordering::Relaxed)
        }
    }
    pub fn prototype(&self, state: &State) -> Option<ObjectPointer> {
        if self.is_tagged_number() {
            Some(state.number_prototype)
        } else {
            self.get().prototype()
        }
    }
    pub fn set_prototype(&self, proto: ObjectPointer) {
        self.get_mut().set_prototype(proto);
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

    /// Adds an attribute to the object this pointer points to.
    pub fn add_attribute(&self, name: &Arc<String>, attr: ObjectPointer) {
        self.get_mut().add_attribute(name.clone(), attr);

        //process.write_barrier(*self, attr);
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(&self, state: &RcState, name: &Arc<String>) -> Option<ObjectPointer> {
        if self.is_tagged_number() {
            state.number_prototype.get().lookup_attribute(name)
        } else {
            self.get().lookup_attribute(name)
        }
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        state: &RcState,
        name: &Arc<String>,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_number() {
            state.number_prototype.get().lookup_attribute_in_self(name)
        } else {
            self.get().lookup_attribute_in_self(name)
        }
    }

    pub fn attributes(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_number() {
            vec![]
        } else {
            self.get().attributes()
        }
    }

    pub fn attribute_names(&self) -> Vec<&Arc<String>> {
        if self.is_tagged_number() {
            vec![]
        } else {
            self.get().attribute_names()
        }
    }

    pub fn mark(&self) {
        if self.is_tagged_number() {
            unreachable!()
        }
        let x = self.get_mut();
        x.marked.store(true, Ordering::Release);
    }

    pub fn unmark(&self) {
        if self.is_tagged_number() {
            unreachable!()
        }
        let x = self.get_mut();
        x.marked.store(true, Ordering::Release);
    }

    pub fn as_string(&self) -> Result<&Arc<String>, String> {
        if self.is_tagged_number() {
            Err(format!("Called ObjectPointer::as_string() on non string"))
        } else {
            match self.get().value {
                ObjectValue::String(ref s) => Ok(s),
                _ => Err(format!("Called ObjectPointer::as_string() on non string")),
            }
        }
    }

    pub fn to_string(&self) -> Arc<String> {
        if self.is_null() {
            panic!();
        }
        if self.is_tagged_number() {
            Arc::new(self.number_value().unwrap().to_string())
        } else {
            match self.get().value {
                ObjectValue::String(ref s) => s.clone(),
                ObjectValue::Array(ref array) => {
                    use std::fmt::Write;
                    let mut fmt_buf = String::new();
                    write!(fmt_buf, "[").unwrap();
                    for (i, object) in array.iter().enumerate() {
                        write!(fmt_buf, "{}", object.to_string()).unwrap();
                        if i != array.len() - 1 {
                            write!(fmt_buf, ",").unwrap();
                        }
                    }
                    write!(fmt_buf, "]").unwrap();

                    Arc::new(fmt_buf)
                }
                ObjectValue::Thread(_) => Arc::new(String::from("Thread")),
                ObjectValue::File(_) => Arc::new(String::from("File")),
                ObjectValue::ByteArray(ref array) => Arc::new(format!("{:?}", array)),
                ObjectValue::Function(_) => Arc::new(String::from("Function")),
                ObjectValue::Number(n) => Arc::new(n.to_string()),
                ObjectValue::Module(_) => Arc::new(String::from("Module")),
                ObjectValue::None => {
                    if self.get().has_attributes() {
                        use std::fmt::Write;
                        let mut fmt_buf = String::new();
                        write!(fmt_buf, "{{\n").unwrap();
                        for (i, (key, value)) in
                            self.get().attributes.as_ref().unwrap().iter().enumerate()
                        {
                            write!(fmt_buf, "  {}: {}\n", key, value.to_string()).unwrap();
                        }

                        Arc::new(fmt_buf)
                    } else {
                        Arc::new(String::from("{}"))
                    }
                }
                ObjectValue::Bool(x) => Arc::new(x.to_string()),
            }
        }
    }

    pub fn number(x: f64) -> Self {
        Self {
            raw: TaggedPointer::with_bit((x.to_bits() << 1) as *mut _, INTEGER_BIT),
        }
    }

    pub fn finalize(&self) {
        if !self.is_finalizable() {
            return;
        }
        unsafe {
            std::ptr::drop_in_place(self.raw.raw);
            std::alloc::dealloc(self.raw.raw as *mut u8, std::alloc::Layout::new::<Object>());
        }
    }

    pub const fn null() -> Self {
        Self {
            raw: TaggedPointer::null(),
        }
    }

    pub fn is_tagged_number(&self) -> bool {
        self.raw.bit_is_set(INTEGER_BIT)
    }

    pub fn number_value(&self) -> Result<f64, String> {
        if self.is_tagged_number() {
            Ok(f64::from_bits(self.raw.raw as u64 >> 1))
        } else if let ObjectValue::Number(ref x) = self.get().value {
            Ok(*x)
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
    pub fn is_finalizable(&self) -> bool {
        !self.is_tagged_number() || self.get().is_finalizable()
    }
}

impl Object {
    /// Returns a new object with the given value.
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: ObjectPointer::null(),
            attributes: TaggedPointer::null(),
            value,
            marked: AtomicBool::new(false),
        }
    }

    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_attributes()
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(value: ObjectValue, prototype: ObjectPointer) -> Object {
        Object {
            prototype,
            attributes: TaggedPointer::null(),
            value,
            marked: AtomicBool::new(false),
        }
    }
    pub fn each_pointer<F: FnMut(ObjectPointerPointer)>(&self, mut callback: F) {
        if !self.prototype.is_null() {
            callback(self.prototype.pointer())
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
    pub fn remove_attribute(&mut self, name: &Arc<String>) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map_mut() {
            map.remove(name)
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
    pub fn attribute_names(&self) -> Vec<&Arc<String>> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            for (key, _) in map.iter() {
                attributes.push(key);
            }
            //push_collection!(map, keys, attributes);
        }

        attributes
    }

    /// Looks up an attribute in either the current object or a parent object.
    pub fn lookup_attribute(&self, name: &Arc<String>) -> Option<ObjectPointer> {
        let got = self.lookup_attribute_in_self(&name);

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
    /// Allocates an attribute map if needed.
    fn allocate_attributes_map(&mut self) {
        if !self.has_attributes() {
            self.set_attributes_map(AttributesMap::default());
        }
    }

    /// Returns true if an attributes map has been allocated.
    pub fn has_attributes(&self) -> bool {
        if self.is_forwarded() {
            return false;
        }

        !self.attributes.untagged().is_null()
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
    /// Adds a new attribute to the current object.
    pub fn add_attribute(&mut self, name: Arc<String>, object: ObjectPointer) {
        self.allocate_attributes_map();

        self.attributes_map_mut().unwrap().insert(name, object);
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(&self, name: &Arc<String>) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map() {
            map.get(name).cloned()
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
}

/// The bit to set for objects that are being forwarded.
pub const PENDING_FORWARD_BIT: usize = 0;

/// The bit to set for objects that have been forwarded.
pub const FORWARDED_BIT: usize = 1;

/// The bit to set for objects stored in a remembered set.
pub const REMEMBERED_BIT: usize = 2;

/// The mask to apply when installing a forwarding pointer.
pub const FORWARDING_MASK: usize = 0x3;

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
use std::hash::{Hash, Hasher as HasherTrait};
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

unsafe impl Sync for ObjectPointerPointer {}
unsafe impl Send for ObjectPointerPointer {}

pub fn new_native_fn(state: &RcState, fun: NativeFn, argc: i32) -> ObjectPointer {
    let function = Function {
        argc,
        upvalues: vec![],
        native: Some(fun),
        code: Ptr::null(),
        module: Arc::new(Module::new()),
        name: Arc::new(String::new()),
        hotness: 0,
    };

    let object = Object::with_prototype(ObjectValue::Function(function), state.function_prototype);
    state.gc.allocate(object)
}

use std::fmt;
impl fmt::Debug for ObjectPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
