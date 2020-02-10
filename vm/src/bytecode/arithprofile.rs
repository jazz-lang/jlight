use crate::types::*;

#[derive(Clone, Copy)]
pub struct ObservedType {
    pub bits: u8,
}

impl ObservedType {
    pub const EMPTY: u8 = 0x0;
    pub const INT32: u8 = 0x1;
    pub const NUMBER: u8 = 0x02;
    pub const NON_NUMBER: u8 = 0x04;
    pub const NUM_BITS_NEEDED: u8 = 3;
    pub const fn new(bits: u8) -> Self {
        Self { bits }
    }

    pub const fn empty() -> Self {
        Self { bits: Self::EMPTY }
    }

    pub const fn saw_int32(&self) -> bool {
        self.bits & Self::INT32 != 0
    }

    pub const fn is_only_int32(&self) -> bool {
        self.bits == Self::INT32
    }

    pub const fn saw_number(&self) -> bool {
        self.bits & Self::NUMBER != 0
    }

    pub const fn is_only_number(&self) -> bool {
        self.bits == Self::NUMBER
    }

    pub const fn is_empty(&self) -> bool {
        !self.bits == 0
    }

    pub const fn with_int32(&self) -> Self {
        Self::new(self.bits | Self::INT32)
    }

    pub const fn with_number(&self) -> Self {
        Self::new(self.bits | Self::NUMBER)
    }

    pub const fn with_non_number(&self) -> Self {
        Self::new(self.bits | Self::NON_NUMBER)
    }

    pub const fn without_non_number(&self) -> Self {
        Self::new(self.bits | !Self::NON_NUMBER)
    }
}
#[derive(Clone, Copy, Default)]
pub struct ObservedResults {
    pub bits: u8,
}

impl ObservedResults {
    pub const NON_NEG_ZERO_DOUBLE: u8 = 1 << 0;
    pub const NEG_ZERO_DOUBLE: u8 = 1 << 1;
    pub const NON_NUMERIC: u8 = 1 << 2;
    pub const INT32_OVERFLOW: u8 = 1 << 3;
    pub const INT52_OVERFLOW: u8 = 1 << 4;
    pub const BIGINT: u8 = 1 << 5;
    pub const NUM_BITS_NEEDED: u16 = 6;
    pub const fn new(x: u8) -> Self {
        Self { bits: x }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum ArithProfileType {
    Binary,
    Unary,
}

pub struct ArithProfile {
    pub ty: ArithProfileType,
    pub bits: u16,
}

impl ArithProfile {
    pub const fn observed_results(&self) -> u16 {
        return self.bits & ((1 << ObservedResults::NUM_BITS_NEEDED) - 1);
    }
}
