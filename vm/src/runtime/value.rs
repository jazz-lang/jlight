use super::cell::*;
use crate::util::arc::Arc;
use std::sync::atomic::Ordering;
pub type EncodedValue = i64;

#[derive(Copy, Clone)]
#[repr(C)]
union EncodedValueDescriptor {
    as_int64: i64,
    #[cfg(feature = "use-value64")]
    ptr: CellPointer,
    as_bits: AsBits,
}

/// TODO: Big endian support
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

#[derive(Copy, Clone)]
pub struct Value {
    u: EncodedValueDescriptor,
}

pub const NOT_INT52: usize = 1 << 52;
impl Value {
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
            /// NOT_CELL_MASK is used to check for all types of immediate values (either number or 'other').
            pub const NOT_CELL_MASK: i64 = Self::NUMBER_TAG | Self::OTHER_TAG as i64;
            pub const VALUE_EMPTY: i32 = 0x0;
            pub const VALUE_DELETED: i32 = 0x4;
        }
    }

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
    pub fn as_cell(&self) -> CellPointer {
        assert!(self.is_cell());
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

impl From<CellPointer> for Value {
    fn from(x: CellPointer) -> Self {
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
