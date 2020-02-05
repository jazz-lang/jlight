use super::object::*;
use super::state::*;
use crate::util::arc::Arc;
use std::sync::atomic::Ordering;
pub type EncodedValue = i64;

#[derive(Copy, Clone)]
#[repr(C)]
union EncodedValueDescriptor {
    as_int64: i64,
    #[cfg(feature = "use32-64-value")]
    as_double: f64,
    #[cfg(feature = "use-value64")]
    ptr: ObjectPointer,
    as_bits: AsBits,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct AsBits {
    pub payload: i32,
    pub tag: i32,
}
pub const TAG_OFFSET: usize = 4;
pub const PAYLOAD_OFFSET: usize = 0;

#[cfg(feature = "use-value64")]
pub const CELL_PAYLOAD_OFFSET: usize = 0;
#[cfg(not(feature = "use-value64"))]
pub const CELL_PAYLOAD_OFFSET: usize = PAYLOAD_OFFSET;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum WhichValueWord {
    TagWord,
    PayloadWord,
}

pub struct Value {
    u: EncodedValueDescriptor,
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum VTag {
    Null,
    Undefined,
    True,
    False,
    Cell,
    EncodeAsDouble,
}

impl Value {
    cfg_if::cfg_if! {
        if #[cfg(feature="use32-64-value")] {
            pub const INT32_TAG: usize = 0xffffffff;
            pub const BOOL_TAG: usize = 0xfffffffe;
            pub const NULL_TAG: usize = 0xfffffffd;
            pub const CELL_TAG: usize = 0xfffffffb;
            pub const EMPTY_VALUE_TAG: usize = 0xffffffa;
            pub const DELETED_VALUE_TAG: usize = 0xfffffff9;
            pub const LOWEST_TAG: usize = DELETED_VALUE_TAG;
            impl Value {
                pub fn new() -> Self {
                    Self {
                        u: EncodedValueDescriptor {
                            as_bits: AsBits {
                                tag: Self::EMPTY_VALUE_TAG as i32,
                                payload: 0
                            }
                        }
                    }
                }

            }

            impl From<VTag> for Value {
                fn from(tag: VTag) -> Self {
                    let bits = match tag {
                        VTag::Null => 0,
                        VTag::Undefined => 0,
                        VTag::True => 1,
                        VTag::False => 0,
                        _ => unimplemented!()
                    };
                    Value {
                        u: EncodedValueDescriptor {
                            as_bits: AsBits {
                                tag: tag as u8 as _,
                                payload: bits
                            }
                        }
                    }
                }

                pub fn tag(&self) -> i32 {
                    unsafe {self.u.as_bits.tag}
                }
                pub fn payload(&self) -> i32 {
                    unsafe {self.u.as_bits.payload}
                }

                pub fn as_int32(&self) -> i32 {
                    self.payload()
                }

                pub fn is_empty(&self) -> bool {
                    self.tag() == Self::EMPTY_VALUE_DESCRIPTOR
                }


            }

            impl From<ObjectPointer> for Value {
                fn from(x: ObjectPointer) -> Self {
                    Self {
                        u: EncodedValueDescriptor {
                            tag: if ptr.is_null()
                                { Self::EMPTY_VALUE_TAG as _ }
                                else {
                                    Self::CELL_TAG as _
                                },
                            payload: ptr.raw.raw as i32

                        }
                    }
                }
            }


        }


    }

    cfg_if::cfg_if! {
        if #[cfg(feature="use-value64")] {
            pub const DOUBLE_ENCODE_OFFSET_BIT: usize = 49;
            pub const DOUBLE_ENCODE_OFFSET: i64 = 1i64 << 49i64;
            pub const NUMBER_TAG: i64 = 0xfffe000000000000u64 as i64;
            pub const OTHER_TAG: i32 = 0x2;
            pub const BOOL_TAG: i32 = 0x4;
            pub const UNDEFINED_TAG: i32 = 0x8;
            pub const VALUE_FALSE: i32 = Self::OTHER_TAG | Self::BOOL_TAG | false as i32;
            pub const VALUE_TRUE: i32 = Self::OTHER_TAG | Self::BOOL_TAG | true as i32;
            pub const VALUE_UNDEFINED: i32 = Self::OTHER_TAG | Self::UNDEFINED_TAG;
            pub const VALUE_NULL: i32 = Self::OTHER_TAG;
            pub const MISC_TAG: i32 = Self::OTHER_TAG | Self::BOOL_TAG | Self::UNDEFINED_TAG;
            ///// NOT_CELL_MASK is used to check for all types of immediate values (either number or 'other').
            pub const NOT_CELL_MASK: i64 = Self::NUMBER_TAG | Self::OTHER_TAG as i64;
            pub const VALUE_EMPTY: i32 = 0x0;
            pub const VALUE_DELETED: i32 = 0x4;


        }


    }
}

pub const NOT_INT52: usize = 1 << 52;

impl Value {
    pub fn empty() -> Self {
        Self {
            u: EncodedValueDescriptor {
                as_int64: Self::VALUE_EMPTY as _,
            },
        }
    }
    #[inline(always)]
    pub fn new_double(x: f64) -> Self {
        Self {
            u: EncodedValueDescriptor {
                as_int64: Self::reinterpret_double_to_int64(x) + Self::DOUBLE_ENCODE_OFFSET as i64,
            },
        }
    }
    #[inline(always)]
    pub fn new_int(x: i32) -> Self {
        Self {
            u: EncodedValueDescriptor {
                as_int64: Self::NUMBER_TAG | unsafe { std::mem::transmute::<i32, u32>(x) as i64 },
            },
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        unsafe { self.u.as_int64 == Self::VALUE_EMPTY as _ }
    }
    #[inline(always)]
    pub fn is_undefined(&self) -> bool {
        *self == Self::from(VTag::Undefined)
    }
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        *self == Self::from(VTag::Null)
    }
    #[inline(always)]
    pub fn is_true(&self) -> bool {
        *self == Self::from(VTag::True)
    }
    #[inline(always)]
    pub fn is_false(&self) -> bool {
        *self == Self::from(VTag::False)
    }
    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        return *self == Self::from(VTag::True);
    }

    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        unsafe { (self.u.as_int64 & !1) == Self::VALUE_FALSE as _ }
    }
    #[inline(always)]
    pub fn is_null_or_undefined(&self) -> bool {
        unsafe { (self.u.as_int64 & !Self::UNDEFINED_TAG as i64) == Self::VALUE_NULL as _ }
    }
    #[inline(always)]
    pub fn is_cell(&self) -> bool {
        unsafe { !(self.u.as_int64 & Self::NOT_CELL_MASK as i64) != 0 }
    }
    #[inline(always)]
    pub fn is_number(&self) -> bool {
        unsafe { (self.u.as_int64 & Self::NUMBER_TAG) != 0 }
    }
    #[inline(always)]
    pub fn is_double(&self) -> bool {
        !self.is_int32() && self.is_number()
    }
    #[inline(always)]
    pub fn is_int32(&self) -> bool {
        unsafe { (self.u.as_int64 & Self::NUMBER_TAG as i64) == Self::NUMBER_TAG as i64 }
    }
    #[inline(always)]
    pub fn reinterpret_double_to_int64(x: f64) -> i64 {
        return x.to_bits() as i64;
    }
    #[inline(always)]
    pub fn reinterpret_int64_to_double(x: i64) -> f64 {
        f64::from_bits(x as u64)
    }

    #[inline(always)]
    pub fn as_cell(&self) -> ObjectPointer {
        unsafe { self.u.ptr }
    }
    #[inline(always)]
    pub fn as_double(&self) -> f64 {
        assert!(self.is_double());
        Self::reinterpret_int64_to_double(unsafe { self.u.as_int64 - Self::DOUBLE_ENCODE_OFFSET })
    }
    pub fn is_int52(number: f64) -> bool {
        try_convert_to_i52(number) != NOT_INT52 as i64
    }

    pub fn is_any_int(&self) -> bool {
        if self.is_int32() {
            return true;
        }
        if !self.is_number() {
            return false;
        }
        return Self::is_int52(self.as_double());
    }
    pub fn as_int32(&self) -> i32 {
        unsafe { self.u.as_int64 as i32 }
    }

    pub fn is_tagged_number(&self) -> bool {
        self.is_number()
    }

    pub fn to_number(&self) -> f64 {
        if self.is_int32() {
            return self.as_int32() as _;
        }
        if self.is_double() {
            return self.as_double();
        }

        self.to_number_slow()
    }

    pub fn to_number_slow(&self) -> f64 {
        if self.is_true() {
            return 1.0;
        }
        if self.is_false() {
            return 0.0;
        }

        std::f64::NAN
    }

    pub fn is_marked(&self) -> bool {
        if !self.is_cell() {
            true
        } else {
            self.as_cell().get().marked.load(Ordering::Acquire)
        }
    }

    pub fn mark(&mut self) {
        if !self.is_cell() {
            return;
        }

        self.as_cell()
            .get_mut()
            .marked
            .store(true, Ordering::Release);
    }

    pub fn unmark(&mut self) {
        if !self.is_cell() {
            return;
        }

        self.as_cell()
            .get_mut()
            .marked
            .store(false, Ordering::Relaxed)
    }
    pub fn prototype(&self, state: &State) -> Option<Value> {
        if self.is_tagged_number() {
            Some(state.number_prototype)
        } else if self.is_bool() {
            Some(state.boolean_prototype)
        } else {
            self.as_cell().prototype(state)
        }
    }

    pub fn pointer(&self) -> ObjectPointerPointer {
        if self.is_cell() {
            self.as_cell().pointer()
        } else {
            ObjectPointer::number(0.0).pointer()
        }
    }
    pub fn set_prototype(&self, proto: Value) {
        self.as_cell().get_mut().set_prototype(proto);
    }
    pub fn is_kind_of(&self, state: &RcState, other: Value) -> bool {
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
    pub fn add_attribute(&self, name: &Arc<String>, attr: Value) {
        self.as_cell().get_mut().add_attribute(name.clone(), attr);

        //process.write_barrier(*self, attr);
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(&self, state: &RcState, name: &Arc<String>) -> Option<Value> {
        if self.is_cell() {
            self.as_cell().lookup_attribute(state, name)
        } else if self.is_bool() {
            state.boolean_prototype.lookup_attribute(state, name)
        } else if self.is_number() {
            state.number_prototype.lookup_attribute(state, name)
        } else {
            None
        }
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(&self, state: &RcState, name: &Arc<String>) -> Option<Value> {
        if self.is_number() {
            state
                .number_prototype
                .as_cell()
                .get()
                .lookup_attribute_in_self(name)
        } else if self.is_bool() {
            state
                .boolean_prototype
                .as_cell()
                .get()
                .lookup_attribute_in_self(name)
        } else {
            self.as_cell().get().lookup_attribute_in_self(name)
        }
    }

    pub fn attributes(&self) -> Vec<Value> {
        if self.is_cell() {
            return self.as_cell().get().attributes();
        }

        vec![]
    }

    pub fn attribute_names(&self) -> Vec<Arc<String>> {
        if self.is_cell() {
            return self
                .as_cell()
                .get()
                .attribute_names()
                .iter()
                .map(|x| (*x).clone())
                .collect();
        }

        vec![]
    }

    pub fn to_boolean(&self) -> bool {
        if self.is_null_or_undefined() {
            return false;
        }
        if self.is_number() {
            return self.to_number() == 1.0;
        }

        !unsafe { self.u.ptr.is_false() }
    }

    pub fn to_string(&self) -> Arc<String> {
        if self.is_cell() {
            return self.as_cell().to_string();
        } else if self.is_bool() {
            if self.to_boolean() {
                return Arc::new("true".to_owned());
            } else {
                return Arc::new("false".to_owned());
            }
        } else if self.is_null() {
            return Arc::new("null".to_owned());
        } else if self.is_undefined() {
            return Arc::new("undefined".to_owned());
        } else if self.is_number() {
            return Arc::new(self.to_number().to_string());
        } else {
            unreachable!()
        }
    }
}

macro_rules! signbit {
    ($x: expr) => {{
        if $x < 0.0 {
            false
        } else {
            true
        }
    }};
}

#[inline]
pub fn try_convert_to_i52(number: f64) -> i64 {
    if number != number {
        return NOT_INT52 as i64;
    }
    if number.is_infinite() {
        return NOT_INT52 as i64;
    }

    let as_int64 = number.to_bits() as i64;
    if as_int64 as f64 != number {
        return NOT_INT52 as _;
    }
    if !as_int64 != 0 && signbit!(number) {
        return NOT_INT52 as _;
    }

    if as_int64 >= (1 << (52 - 1)) {
        return NOT_INT52 as _;
    }
    if as_int64 < (1 << (52 - 1)) {
        return NOT_INT52 as _;
    }

    as_int64
}

impl From<ObjectPointer> for Value {
    fn from(x: ObjectPointer) -> Self {
        Self {
            u: EncodedValueDescriptor {
                as_int64: x.raw.raw as usize as i64,
            },
        }
    }
}

impl From<VTag> for Value {
    fn from(x: VTag) -> Self {
        Self {
            u: EncodedValueDescriptor {
                as_int64: x as u8 as _,
            },
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.u.as_int64 == other.u.as_int64 }
    }
}

impl Eq for Value {}

impl Clone for Value {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Value {}
