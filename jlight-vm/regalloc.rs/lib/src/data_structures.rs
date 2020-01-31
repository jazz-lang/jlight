/* -*- Mode: Rust; tab-width: 8; indent-tabs-mode: nil; rust-indent-offset: 2 -*-
 * vim: set ts=8 sts=2 et sw=2 tw=80:
*/

//! Data structures for the whole crate.

use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::Index;
use std::ops::IndexMut;
use std::slice::{Iter, IterMut};

use crate::interface::Function;

//=============================================================================
// Queues

pub type Queue<T> = VecDeque<T>;

//=============================================================================
// Maps

// NOTE: plain HashMap is nondeterministic, even in a single-threaded
// scenario, which can make debugging code that uses it really confusing.  So
// we use FxHashMap instead, as it *is* deterministic, and, allegedly, faster
// too.
pub type Map<K, V> = FxHashMap<K, V>;

//=============================================================================
// Sets of things

// Same comment as above for FxHashMap.
pub struct Set<T> {
  set: FxHashSet<T>,
}

impl<T: Eq + Ord + Hash + Copy + fmt::Debug> Set<T> {
  pub fn empty() -> Self {
    Self { set: FxHashSet::<T>::default() }
  }

  pub fn unit(item: T) -> Self {
    let mut s = Self::empty();
    s.insert(item);
    s
  }

  pub fn two(item1: T, item2: T) -> Self {
    let mut s = Self::empty();
    s.insert(item1);
    s.insert(item2);
    s
  }

  pub fn card(&self) -> usize {
    self.set.len()
  }

  pub fn insert(&mut self, item: T) {
    self.set.insert(item);
  }

  pub fn is_empty(&self) -> bool {
    self.set.is_empty()
  }

  pub fn contains(&self, item: T) -> bool {
    self.set.contains(&item)
  }

  pub fn intersect(&mut self, other: &Self) {
    let mut res = FxHashSet::<T>::default();
    for item in self.set.iter() {
      if other.set.contains(item) {
        res.insert(*item);
      }
    }
    self.set = res;
  }

  pub fn union(&mut self, other: &Self) {
    for item in other.set.iter() {
      self.set.insert(*item);
    }
  }

  pub fn remove(&mut self, other: &Self) {
    for item in other.set.iter() {
      self.set.remove(item);
    }
  }

  pub fn intersects(&self, other: &Self) -> bool {
    !self.set.is_disjoint(&other.set)
  }

  pub fn is_subset_of(&self, other: &Self) -> bool {
    self.set.is_subset(&other.set)
  }

  pub fn to_vec(&self) -> Vec<T> {
    let mut res = Vec::<T>::new();
    for item in self.set.iter() {
      res.push(*item)
    }
    res.sort_unstable();
    res
  }

  pub fn from_vec(vec: Vec<T>) -> Self {
    let mut res = Set::<T>::empty();
    for x in vec {
      res.insert(x);
    }
    res
  }

  pub fn equals(&self, other: &Self) -> bool {
    self.set == other.set
  }
}

impl<T: Eq + Ord + Hash + Copy + fmt::Debug> fmt::Debug for Set<T> {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "{:?}", self.set)
  }
}

impl<T: Eq + Ord + Hash + Copy + Clone + fmt::Debug> Clone for Set<T> {
  fn clone(&self) -> Self {
    let mut res = Set::<T>::empty();
    for item in self.set.iter() {
      res.set.insert(item.clone());
    }
    res
  }
}

pub struct SetIter<'a, T> {
  set_iter: std::collections::hash_set::Iter<'a, T>,
}
impl<T> Set<T> {
  pub fn iter(&self) -> SetIter<T> {
    SetIter { set_iter: self.set.iter() }
  }
}
impl<'a, T> Iterator for SetIter<'a, T> {
  type Item = &'a T;
  fn next(&mut self) -> Option<Self::Item> {
    self.set_iter.next()
  }
}

//=============================================================================
// Iteration boilerplate for entities.  The only purpose of this is to support
// constructions of the form
//
//   for ent in startEnt .dotdot( endPlus1Ent ) {
//   }
//
// until such time as |trait Step| is available in stable Rust.  At that point
// |fn dotdot| and all of the following can be removed, and the loops
// rewritten using the standard syntax:
//
//   for ent in startEnt .. endPlus1Ent {
//   }

pub trait Zero {
  fn zero() -> Self;
}

pub trait PlusOne {
  fn plus_one(&self) -> Self;
}

pub trait PlusN: PlusOne {
  fn plus_n(&self, n: usize) -> Self;
}

#[derive(Clone, Copy)]
pub struct MyRange<T> {
  first: T,
  last_plus1: T,
  len: usize,
}
impl<T: Copy + PartialOrd + PlusOne> IntoIterator for MyRange<T> {
  type Item = T;
  type IntoIter = MyIterator<T>;
  fn into_iter(self) -> Self::IntoIter {
    MyIterator { range: self, next: self.first }
  }
}

impl<T: Copy + Eq + Ord + PlusOne + PlusN> MyRange<T> {
  /// Create a new range object.
  pub fn new(from: T, len: usize) -> MyRange<T> {
    MyRange { first: from, last_plus1: from.plus_n(len), len }
  }

  pub fn start(&self) -> T {
    self.first
  }

  pub fn first(&self) -> T {
    assert!(self.len() > 0);
    self.start()
  }

  pub fn last(&self) -> T {
    assert!(self.len() > 0);
    self.start().plus_n(self.len() - 1)
  }

  pub fn len(&self) -> usize {
    self.len
  }

  pub fn contains(&self, t: T) -> bool {
    t >= self.first && t < self.first.plus_n(self.len)
  }
}

pub struct MyIterator<T> {
  range: MyRange<T>,
  next: T,
}
impl<T: Copy + PartialOrd + PlusOne> Iterator for MyIterator<T> {
  type Item = T;
  fn next(&mut self) -> Option<Self::Item> {
    if self.next >= self.range.last_plus1 {
      None
    } else {
      let res = Some(self.next);
      self.next = self.next.plus_one();
      res
    }
  }
}

//=============================================================================
// Vectors where both the index and element types can be specified (and at
// most 2^32-1 elems can be stored.  What if this overflows?)

pub struct TypedIxVec<TyIx, Ty> {
  vek: Vec<Ty>,
  ty_ix: PhantomData<TyIx>,
}
impl<TyIx, Ty> TypedIxVec<TyIx, Ty>
where
  Ty: Clone,
  TyIx: Copy + Eq + Ord + Zero + PlusOne + PlusN,
{
  pub fn new() -> Self {
    Self { vek: Vec::new(), ty_ix: PhantomData::<TyIx> }
  }
  pub fn from_vec(vek: Vec<Ty>) -> Self {
    Self { vek, ty_ix: PhantomData::<TyIx> }
  }
  pub fn append(&mut self, other: &mut TypedIxVec<TyIx, Ty>) {
    // FIXME what if this overflows?
    self.vek.append(&mut other.vek);
  }
  pub fn iter(&self) -> Iter<Ty> {
    self.vek.iter()
  }
  pub fn iter_mut(&mut self) -> IterMut<Ty> {
    self.vek.iter_mut()
  }
  pub fn len(&self) -> u32 {
    // FIXME what if this overflows?
    self.vek.len() as u32
  }
  pub fn push(&mut self, item: Ty) {
    // FIXME what if this overflows?
    self.vek.push(item);
  }
  pub fn resize(&mut self, new_len: u32, value: Ty) {
    self.vek.resize(new_len as usize, value);
  }
  pub fn elems(&self) -> &[Ty] {
    &self.vek[..]
  }
  pub fn elems_mut(&mut self) -> &mut [Ty] {
    &mut self.vek[..]
  }
  pub fn range(&self) -> MyRange<TyIx> {
    MyRange::new(TyIx::zero(), self.len() as usize)
  }
}

impl<TyIx, Ty> Index<TyIx> for TypedIxVec<TyIx, Ty>
where
  TyIx: Into<u32>,
{
  type Output = Ty;
  fn index(&self, ix: TyIx) -> &Ty {
    &self.vek[ix.into() as usize]
  }
}

impl<TyIx, Ty> IndexMut<TyIx> for TypedIxVec<TyIx, Ty>
where
  TyIx: Into<u32>,
{
  fn index_mut(&mut self, ix: TyIx) -> &mut Ty {
    &mut self.vek[ix.into() as usize]
  }
}

impl<TyIx, Ty> Clone for TypedIxVec<TyIx, Ty>
where
  Ty: Clone,
{
  // This is only needed for debug printing.
  fn clone(&self) -> Self {
    Self { vek: self.vek.clone(), ty_ix: PhantomData::<TyIx> }
  }
}

//=============================================================================

macro_rules! generate_boilerplate {
  ($TypeIx:ident, $Type:ident, $PrintingPrefix:expr) => {
    #[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
    // Firstly, the indexing type (TypeIx)
    pub enum $TypeIx {
      $TypeIx(u32),
    }
    impl $TypeIx {
      #[allow(dead_code)]
      pub fn new(n: u32) -> Self {
        Self::$TypeIx(n)
      }
      #[allow(dead_code)]
      pub fn max_value() -> Self {
        Self::$TypeIx(u32::max_value())
      }
      #[allow(dead_code)]
      pub fn min_value() -> Self {
        Self::$TypeIx(u32::min_value())
      }
      #[allow(dead_code)]
      pub fn get(self) -> u32 {
        match self {
          $TypeIx::$TypeIx(n) => n,
        }
      }
      #[allow(dead_code)]
      pub fn plus(self, delta: u32) -> $TypeIx {
        $TypeIx::$TypeIx(self.get() + delta)
      }
      #[allow(dead_code)]
      pub fn minus(self, delta: u32) -> $TypeIx {
        $TypeIx::$TypeIx(self.get() - delta)
      }
      #[allow(dead_code)]
      pub fn dotdot(&self, last_plus1: $TypeIx) -> MyRange<$TypeIx> {
        let len = (last_plus1.get() - self.get()) as usize;
        MyRange::new(*self, len)
      }
    }
    impl fmt::Debug for $TypeIx {
      fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}{}", $PrintingPrefix, &self.get())
      }
    }
    impl PlusOne for $TypeIx {
      fn plus_one(&self) -> Self {
        self.plus(1)
      }
    }
    impl PlusN for $TypeIx {
      fn plus_n(&self, n: usize) -> Self {
        self.plus(n as u32)
      }
    }
    impl Into<u32> for $TypeIx {
      fn into(self) -> u32 {
        self.get()
      }
    }
    impl Zero for $TypeIx {
      fn zero() -> Self {
        $TypeIx::new(0)
      }
    }
  };
}

generate_boilerplate!(InstIx, Inst, "i");

generate_boilerplate!(BlockIx, Block, "b");

generate_boilerplate!(RangeFragIx, RangeFrag, "f");

generate_boilerplate!(VirtualRangeIx, VirtualRange, "vr");

generate_boilerplate!(RealRangeIx, RealRange, "rr");

impl<TyIx, Ty: fmt::Debug> fmt::Debug for TypedIxVec<TyIx, Ty> {
  // This is something of a hack in the sense that it doesn't show the
  // indices, but oh well ..
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "{:?}", self.vek)
  }
}

//=============================================================================
// Definitions of register classes, registers and stack slots, and printing
// thereof. Note that this register class definition is meant to be
// architecture-independent: it simply captures common integer/float/vector
// types that machines are likely to use. TODO: investigate whether we need a
// more flexible register-class definition mechanism.

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum RegClass {
  I32,
  F32,
  I64,
  F64,
  V128,
}

/// The number of register classes that exist.
/// N.B.: must be <= 7 (fit into 3 bits) for 32-bit VReg/RReg packed format!
pub const NUM_REG_CLASSES: usize = 5;

impl RegClass {
  /// Convert a register class to a u32 index.
  pub fn rc_to_u32(self) -> u32 {
    match self {
      RegClass::I32 => 0,
      RegClass::F32 => 1,
      RegClass::I64 => 2,
      RegClass::F64 => 3,
      RegClass::V128 => 4,
    }
  }
  /// Convert a register class to a usize index.
  pub fn rc_to_usize(self) -> usize {
    self.rc_to_u32() as usize
  }
  /// Construct a register class from a u32.
  pub fn rc_from_u32(rc: u32) -> RegClass {
    match rc {
      0 => RegClass::I32,
      1 => RegClass::F32,
      2 => RegClass::I64,
      3 => RegClass::F64,
      4 => RegClass::V128,
      _ => panic!("rc_from_u32"),
    }
  }

  pub fn short_name(self) -> &'static str {
    match self {
      RegClass::I32 => "I",
      RegClass::I64 => "J",
      RegClass::F32 => "F",
      RegClass::F64 => "D",
      RegClass::V128 => "V",
    }
  }
}

// Reg represents both real and virtual registers.  For compactness and speed,
// these fields are packed into a single u32.  The format is:
//
// Virtual Reg:   1  rc:3                index:28
// Real Reg:      0  rc:3  uu:12  enc:8  index:8
//
// |rc| is the register class.  |uu| means "unused".  |enc| is the hardware
// encoding for the reg.  |index| is a zero based index which has the
// following meanings:
//
// * for a Virtual Reg, |index| is just the virtual register number.
// * for a Real Reg, |index| is the entry number in the associated
//   |RealRegUniverse|.
//
// This scheme gives us:
//
// * a compact (32-bit) representation for registers
// * fast equality tests for registers
// * ability to handle up to 2^28 (268.4 million) virtual regs per function
// * ability to handle up to 8 register classes
// * ability to handle targets with up to 256 real registers
// * ability to emit instructions containing real regs without having to
//   look up encodings in any side tables, since a real reg carries its
//   encoding
// * efficient bitsets and arrays of virtual registers, since each has a
//   zero-based index baked in
// * efficient bitsets and arrays of real registers, for the same reason
//
// This scheme makes it impossible to represent overlapping register classes,
// but that doesn't seem important.  AFAIK only ARM32 VFP/Neon has that.

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reg {
  do_not_access_this_directly: u32,
}
impl Reg {
  pub fn is_virtual(self) -> bool {
    (self.do_not_access_this_directly & 0x8000_0000) != 0
  }
  pub fn is_real(self) -> bool {
    !self.is_virtual()
  }
  pub fn new_real(rc: RegClass, enc: u8, index: u8) -> Self {
    let n = (0 << 31)
      | (rc.rc_to_u32() << 28)
      | ((enc as u32) << 8)
      | ((index as u32) << 0);
    Reg { do_not_access_this_directly: n }
  }
  pub fn new_virtual(rc: RegClass, index: u32) -> Self {
    if index >= (1 << 28) {
      panic!("new_virtual(): index too large");
    }
    let n = (1 << 31) | (rc.rc_to_u32() << 28) | (index << 0);
    Reg { do_not_access_this_directly: n }
  }
  pub fn get_class(self) -> RegClass {
    RegClass::rc_from_u32((self.do_not_access_this_directly >> 28) & 0x7)
  }
  pub fn get_index(self) -> usize {
    // Return type is usize because typically we will want to use the
    // result for indexing into a Vec
    if self.is_virtual() {
      (self.do_not_access_this_directly & ((1 << 28) - 1)) as usize
    } else {
      (self.do_not_access_this_directly & ((1 << 8) - 1)) as usize
    }
  }
  pub fn get_hw_encoding(self) -> u8 {
    if self.is_virtual() {
      panic!("Virtual register does not have a hardware encoding")
    } else {
      ((self.do_not_access_this_directly >> 8) & ((1 << 8) - 1)) as u8
    }
  }
  pub fn as_virtual_reg(self) -> Option<VirtualReg> {
    if self.is_virtual() {
      Some(VirtualReg { reg: self })
    } else {
      None
    }
  }
}
impl fmt::Debug for Reg {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(
      fmt,
      "{}{}{}",
      if self.is_virtual() { "v" } else { "r" },
      self.get_index(),
      self.get_class().short_name(),
    )
  }
}

// RealReg and VirtualReg are merely wrappers around Reg, which try to
// dynamically ensure that they are really wrapping the correct flavour of
// register.

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RealReg {
  reg: Reg,
}
impl Reg /* !!not RealReg!! */ {
  pub fn to_real_reg(self) -> RealReg {
    if self.is_virtual() {
      panic!("Reg::to_real_reg: this is a virtual register")
    } else {
      RealReg { reg: self }
    }
  }
}
impl RealReg {
  pub fn get_class(self) -> RegClass {
    self.reg.get_class()
  }
  pub fn get_index(self) -> usize {
    self.reg.get_index()
  }
  pub fn get_hw_encoding(self) -> usize {
    self.reg.get_hw_encoding() as usize
  }
  pub fn to_reg(self) -> Reg {
    self.reg
  }
}
impl fmt::Debug for RealReg {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "{:?}", self.reg)
  }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd)]
pub struct VirtualReg {
  reg: Reg,
}
impl Reg /* !!not VirtualReg!! */ {
  pub fn to_virtual_reg(self) -> VirtualReg {
    if self.is_virtual() {
      VirtualReg { reg: self }
    } else {
      panic!("Reg::to_virtual_reg: this is a real register")
    }
  }
}
impl VirtualReg {
  pub fn get_class(self) -> RegClass {
    self.reg.get_class()
  }
  pub fn get_index(self) -> usize {
    self.reg.get_index()
  }
  pub fn to_reg(self) -> Reg {
    self.reg
  }
}
impl fmt::Debug for VirtualReg {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "{:?}", self.reg)
  }
}

impl Reg {
  // Apply a vreg-rreg mapping to a Reg.  This used for registers used in
  // either a read- or a write-role.
  pub fn apply_defs_or_uses(&mut self, map: &Map<VirtualReg, RealReg>) {
    if let Some(vreg) = self.as_virtual_reg() {
      if let Some(rreg) = map.get(&vreg) {
        debug_assert!(rreg.get_class() == vreg.get_class());
        *self = rreg.to_reg();
      } else {
        panic!("Reg::apply_defs_or_uses: no mapping for {:?}", self);
      }
    }
  }
  // Apply a pair of vreg-rreg mappings to a Reg.  The mappings *must*
  // agree!  This seems a bit strange at first.  It is used for registers
  // used in a modify-role.
  pub fn apply_mods(
    &mut self, map_defs: &Map<VirtualReg, RealReg>,
    map_uses: &Map<VirtualReg, RealReg>,
  ) {
    if let Some(vreg) = self.as_virtual_reg() {
      let mb_result_def = map_defs.get(&vreg);
      let mb_result_use = map_uses.get(&vreg);
      // Failure of this is serious and should be investigated.
      if mb_result_def != mb_result_use {
        panic!(
          "Reg::apply_mods: inconsistent mappings for {:?}: D={:?}, U={:?}",
          vreg, mb_result_def, mb_result_use
        );
      }
      if let Some(rreg) = mb_result_def {
        debug_assert!(rreg.get_class() == vreg.get_class());
        *self = rreg.to_reg();
      } else {
        panic!("Reg::apply: no mapping for {:?}", vreg);
      }
    }
  }
}

#[derive(Copy, Clone)]
pub enum SpillSlot {
  SpillSlot(u32),
}
impl SpillSlot {
  pub fn new(n: u32) -> Self {
    SpillSlot::SpillSlot(n)
  }
  pub fn get(self) -> u32 {
    match self {
      SpillSlot::SpillSlot(n) => n,
    }
  }
  pub fn get_usize(self) -> usize {
    self.get() as usize
  }
  pub fn round_up(self, num_slots: u32) -> SpillSlot {
    assert!(num_slots > 0);
    SpillSlot::new((self.get() + num_slots - 1) / num_slots * num_slots)
  }
  pub fn inc(self, num_slots: u32) -> SpillSlot {
    SpillSlot::new(self.get() + num_slots)
  }
}
impl fmt::Debug for SpillSlot {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "S{}", self.get())
  }
}

//=============================================================================
// Definitions of the "real register universe".

// A "Real Register Universe" is a read-only structure that contains all
// information about real registers on a given host.  It serves several
// purposes:
//
// * defines the mapping from real register indices to the registers
//   themselves
//
// * defines the size of the initial section of that mapping that is available
// to the register allocator for use, so that it can treat the registers under
// its control as a zero based, contiguous array.  This is important for its
// efficiency.
//
// * gives meaning to Set<RealReg>, which otherwise would merely be a bunch of
//   bits.

pub struct RealRegUniverse {
  // The registers themselves.  All must be real registers, and all must
  // have their index number (.get_index()) equal to the array index here,
  // since this is the only place where we map index numbers to actual
  // registers.
  pub regs: Vec<(RealReg, String)>,

  // This is the size of the initial section of |regs| that is available to
  // the allocator.  It must be < |regs|.len().
  pub allocable: usize,

  // Ranges for groups of allocable registers. Used to quickly address only
  // a group of allocable registers belonging to the same register class.
  // Indexes into |allocable_by_class| are RegClass values, such as
  // RegClass::F32. If the resulting entry is |None| then there are no
  // registers in that class.  Otherwise the value is |Some((first, last)),
  // which specifies the range of entries in |regs| corresponding to that
  // class.  The range includes both |first| and |last|.
  //
  // In all cases, |last| must be < |allocable|.  In other words,
  // |allocable_by_class| must describe only the allocable prefix of |regs|.
  //
  // For example, let's say
  //    allocable_by_class[RegClass::F32] == Some((10, 14))
  // Then regs[10], regs[11], regs[12], regs[13], and regs[14] give all
  // registers of register class RegClass::F32.
  //
  // The effect of the above is that registers in |regs| must form
  // contiguous groups. This is checked by RealRegUniverse::check_is_sane().
  pub allocable_by_class: [Option<(usize, usize)>; NUM_REG_CLASSES],
}

impl RealRegUniverse {
  /// Check that the given universe satisfies various invariants, and panic
  /// if not.  All the invariants are important.
  pub fn check_is_sane(&self) {
    let regs_len = self.regs.len();
    let regs_allocable = self.allocable;
    // The universe must contain at most 256 registers.  That's because
    // |Reg| only has an 8-bit index value field, so if the universe
    // contained more than 256 registers, we'd never be able to index into
    // entries 256 and above.  This is no limitation in practice since all
    // targets we're interested in contain (many) fewer than 256 regs in
    // total.
    let mut ok = regs_len <= 256;
    // The number of allocable registers must not exceed the number of
    // |regs| presented.  In general it will be less, since the universe
    // will list some registers (stack pointer, etc) which are not
    // available for allocation.
    if ok {
      ok = regs_allocable <= regs_len;
    }
    // All registers must have an index value which points back at the
    // |regs| slot they are in.  Also they really must be real regs.
    if ok {
      for i in 0..regs_len {
        let (reg, _name) = &self.regs[i];
        if ok && (reg.to_reg().is_virtual() || reg.get_index() != i) {
          ok = false;
        }
      }
    }
    // The allocatable regclass groupings defined by |allocable_first| and
    // |allocable_last| must be contiguous.
    if ok {
      let mut regclass_used = [false; NUM_REG_CLASSES];
      for rc in 0..NUM_REG_CLASSES {
        regclass_used[rc] = false;
      }
      for i in 0..regs_allocable {
        let (reg, _name) = &self.regs[i];
        let rc = reg.get_class().rc_to_u32() as usize;
        regclass_used[rc] = true;
      }
      // Scan forward through each grouping, checking that the listed
      // registers really are of the claimed class.  Also count the
      // total number visited.  This seems a fairly reliable way to
      // ensure that the groupings cover all allocated registers exactly
      // once, and that all classes are contiguous groups.
      let mut regs_visited = 0;
      for rc in 0..NUM_REG_CLASSES {
        match self.allocable_by_class[rc] {
          None => {
            if regclass_used[rc] {
              ok = false;
            }
          }
          Some((first, last)) => {
            if !regclass_used[rc] {
              ok = false;
            }
            if ok {
              for i in first..last + 1 {
                let (reg, _name) = &self.regs[i];
                if ok && RegClass::rc_from_u32(rc as u32) != reg.get_class() {
                  ok = false;
                }
                regs_visited += 1;
              }
            }
          }
        }
      }
      if ok && regs_visited != regs_allocable {
        ok = false;
      }
    }
    // So finally ..
    if !ok {
      panic!("RealRegUniverse::check_is_sane: invalid RealRegUniverse");
    }
  }
}

//=============================================================================
// Representing and printing of live range fragments.

#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord)]
// There are four "points" within an instruction that are of interest, and
// these have a total ordering: R < U < D < S.  They are:
//
// * R(eload): this is where any reload insns for the insn itself are
//   considered to live.
//
// * U(se): this is where the insn is considered to use values from those of
//   its register operands that appear in a Read or Modify role.
//
// * D(ef): this is where the insn is considered to define new values for
//   those of its register operands that appear in a Write or Modify role.
//
// * S(pill): this is where any spill insns for the insn itself are considered
//   to live.
//
// Instructions in the incoming Func may only exist at the U and D points,
// and so their associated live range fragments will only mention the U and D
// points.  However, when adding spill code, we need a way to represent live
// ranges involving the added spill and reload insns, in which case R and S
// come into play:
//
// * A reload for instruction i is considered to be live from i.R to i.U.
//
// * A spill for instruction i is considered to be live from i.D to i.S.
pub enum Point {
  Reload,
  Use,
  Def,
  Spill,
}
impl Point {
  pub fn min_value() -> Self {
    Self::Reload
  }
  pub fn max_value() -> Self {
    Self::Spill
  }
  pub fn is_reload(self) -> bool {
    match self {
      Point::Reload => true,
      _ => false,
    }
  }
  pub fn is_use(self) -> bool {
    match self {
      Point::Use => true,
      _ => false,
    }
  }
  pub fn is_def(self) -> bool {
    match self {
      Point::Def => true,
      _ => false,
    }
  }
  pub fn is_spill(self) -> bool {
    match self {
      Point::Spill => true,
      _ => false,
    }
  }
  pub fn is_use_or_def(self) -> bool {
    self.is_use() || self.is_def()
  }
}
impl PartialOrd for Point {
  // In short .. R < U < D < S.  This is probably what would be #derive'd
  // anyway, but we need to be sure.
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    // This is a bit idiotic, but hey .. hopefully LLVM can turn it into a
    // no-op.
    fn convert(pt: &Point) -> u32 {
      match pt {
        Point::Reload => 0,
        Point::Use => 1,
        Point::Def => 2,
        Point::Spill => 3,
      }
    }
    convert(self).partial_cmp(&convert(other))
  }
}

// See comments below on |RangeFrag| for the meaning of |InstPoint|.
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord)]
pub struct InstPoint {
  pub iix: InstIx,
  pub pt: Point,
}
impl InstPoint {
  pub fn new(iix: InstIx, pt: Point) -> Self {
    InstPoint { iix, pt }
  }
  pub fn new_reload(iix: InstIx) -> Self {
    InstPoint { iix, pt: Point::Reload }
  }
  pub fn new_use(iix: InstIx) -> Self {
    InstPoint { iix, pt: Point::Use }
  }
  pub fn new_def(iix: InstIx) -> Self {
    InstPoint { iix, pt: Point::Def }
  }
  pub fn new_spill(iix: InstIx) -> Self {
    InstPoint { iix, pt: Point::Spill }
  }
}
impl PartialOrd for InstPoint {
  // Again .. don't assume anything about the #derive'd version.  These have
  // to be ordered using |iix| as the primary key and |pt| as the
  // secondary.
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    match self.iix.partial_cmp(&other.iix) {
      Some(Ordering::Less) => Some(Ordering::Less),
      Some(Ordering::Greater) => Some(Ordering::Greater),
      Some(Ordering::Equal) => self.pt.partial_cmp(&other.pt),
      None => panic!("InstPoint::partial_cmp: fail #1"),
    }
  }
}

impl fmt::Debug for InstPoint {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(
      fmt,
      "{:?}{}",
      self.iix,
      match self.pt {
        Point::Reload => "/r",
        Point::Use => "/u",
        Point::Def => "/d",
        Point::Spill => "/s",
      }
    )
  }
}

impl InstPoint {
  pub fn max_value() -> Self {
    Self { iix: InstIx::max_value(), pt: Point::max_value() }
  }
  pub fn min_value() -> Self {
    Self { iix: InstIx::min_value(), pt: Point::min_value() }
  }
}

// A handy summary hint for a RangeFrag.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RangeFragKind {
  Local,   // Fragment exists entirely inside one block
  LiveIn,  // Fragment is live in to a block, but ends inside it
  LiveOut, // Fragment is live out of a block, but starts inside it
  Thru,    // Fragment is live through the block (starts and ends outside it)
}
impl fmt::Debug for RangeFragKind {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    match self {
      RangeFragKind::Local => write!(fmt, "Local"),
      RangeFragKind::LiveIn => write!(fmt, "LiveIn"),
      RangeFragKind::LiveOut => write!(fmt, "LiveOut"),
      RangeFragKind::Thru => write!(fmt, "Thru"),
    }
  }
}

//=============================================================================
// Metrics.  Meaning, estimated hotness, etc, numbers, which don't have any
// effect on the correctness of the resulting allocation, but which are
// important for getting a good allocation, basically by giving preference for
// the hottest values getting a register.

/* Required metrics:
   Block (a basic block):
   - Estimated relative execution frequency ("EEF")
     Calculated from loop nesting depth, depth inside an if-tree, etc
     Suggested: u16

   RangeFrag (Live Range Fragment):
   - Length (in instructions).  Can be calculated, = end - start + 1.
   - Number of uses (of the associated Reg)
     Suggested: u16

   LR (Live Range, = a set of Live Range Fragments):
   - spill cost (can be calculated)
     = sum, for each frag:
            frag.#uses / frag.len * frag.block.estFreq
       with the proviso that spill/reload LRs must have spill cost of infinity
     Do this with a f32 so we don't have to worry about scaling/overflow.
*/

// A Live Range Fragment (RangeFrag) describes a consecutive sequence of one or
// more instructions, in which a Reg is "live".  The sequence must exist
// entirely inside only one basic block.
//
// However, merely indicating the start and end instruction numbers isn't
// enough: we must also include a "Use or Def" indication.  These indicate two
// different "points" within each instruction: the Use position, where
// incoming registers are read, and the Def position, where outgoing registers
// are written.  The Use position is considered to come before the Def
// position, as described for |Point| above.
//
// When we come to generate spill/restore live ranges, Point::S and Point::R
// also come into play.  Live ranges (and hence, RangeFrags) that do not perform
// spills or restores should not use either of Point::S or Point::R.
//
// The set of positions denoted by
//
//    {0 .. #insns-1} x {Reload point, Use point, Def point, Spill point}
//
// is exactly the set of positions that we need to keep track of when mapping
// live ranges to registers.  This the reason for the type InstPoint.  Note
// that InstPoint values have a total ordering, at least within a single basic
// block: the insn number is used as the primary key, and the Point part is
// the secondary key, with Reload < Use < Def < Spill.
//
// Finally, a RangeFrag has a |count| field, which is a u16 indicating how often
// the associated storage unit (Reg) is mentioned inside the RangeFrag.  It is
// assumed that the RangeFrag is associated with some Reg.  If not, the |count|
// field is meaningless.
//
// The |bix| field is actually redundant, since the containing |Block| can be
// inferred, laboriously, from |first| and |last|, providing you have a
// |Block| table to hand.  It is included here for convenience.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RangeFrag {
  pub bix: BlockIx,
  pub kind: RangeFragKind,
  pub first: InstPoint,
  pub last: InstPoint,
  pub count: u16,
}
impl fmt::Debug for RangeFrag {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(
      fmt,
      "{:?}; count={}; {:?} [{:?} {:?}]",
      self.bix, self.count, self.kind, self.first, self.last
    )
  }
}
impl RangeFrag {
  pub fn new<F: Function>(
    f: &F, bix: BlockIx, first: InstPoint, last: InstPoint, count: u16,
  ) -> Self {
    debug_assert!(f.block_insns(bix).len() >= 1);
    debug_assert!(f.block_insns(bix).contains(first.iix));
    debug_assert!(f.block_insns(bix).contains(last.iix));
    debug_assert!(first <= last);
    if first == last {
      debug_assert!(count == 1);
    }
    let first_iix_in_block = f.block_insns(bix).first();
    let last_iix_in_block = f.block_insns(bix).last();
    let first_pt_in_block = InstPoint::new_use(first_iix_in_block);
    let last_pt_in_block = InstPoint::new_def(last_iix_in_block);
    let kind = match (first == first_pt_in_block, last == last_pt_in_block) {
      (false, false) => RangeFragKind::Local,
      (false, true) => RangeFragKind::LiveOut,
      (true, false) => RangeFragKind::LiveIn,
      (true, true) => RangeFragKind::Thru,
    };
    RangeFrag { bix, kind, first, last, count }
  }
}

// Comparison of RangeFrags.  They form a partial order.

pub fn cmp_range_frags(f1: &RangeFrag, f2: &RangeFrag) -> Option<Ordering> {
  if f1.last < f2.first {
    return Some(Ordering::Less);
  }
  if f1.first > f2.last {
    return Some(Ordering::Greater);
  }
  if f1.first == f2.first && f1.last == f2.last {
    return Some(Ordering::Equal);
  }
  None
}
impl RangeFrag {
  pub fn contains(&self, ipt: &InstPoint) -> bool {
    self.first <= *ipt && *ipt <= self.last
  }
}

//=============================================================================
// Vectors of RangeFragIxs, sorted so that the associated RangeFrags are in
// ascending order (per their InstPoint fields).
//
// The "fragment environment" (sometimes called 'fenv' or 'frag_env') to which
// the RangeFragIxs refer, is not stored here.

#[derive(Clone)]
pub struct SortedRangeFragIxs {
  pub frag_ixs: Vec<RangeFragIx>,
}
impl fmt::Debug for SortedRangeFragIxs {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    self.frag_ixs.fmt(fmt)
  }
}
impl SortedRangeFragIxs {
  pub fn show_with_fenv(
    &self, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) -> String {
    let mut frags = TypedIxVec::<RangeFragIx, RangeFrag>::new();
    for fix in &self.frag_ixs {
      frags.push(fenv[*fix]);
    }
    format!("SFIxs_{:?}", &frags)
  }

  fn check(&self, fenv: &TypedIxVec<RangeFragIx, RangeFrag>) {
    let mut ok = true;
    for i in 1..self.frag_ixs.len() {
      let prev_frag = &fenv[self.frag_ixs[i - 1]];
      let this_frag = &fenv[self.frag_ixs[i - 0]];
      if cmp_range_frags(prev_frag, this_frag) != Some(Ordering::Less) {
        ok = false;
        break;
      }
    }
    if !ok {
      panic!("SortedRangeFragIxs::check: vector not ok");
    }
  }

  pub fn new(
    source: &Vec<RangeFragIx>, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) -> Self {
    let mut res = SortedRangeFragIxs { frag_ixs: source.clone() };
    // check the source is ordered, and clone (or sort it)
    res.frag_ixs.sort_unstable_by(|fix_a, fix_b| {
      match cmp_range_frags(&fenv[*fix_a], &fenv[*fix_b]) {
        Some(Ordering::Less) => Ordering::Less,
        Some(Ordering::Greater) => Ordering::Greater,
        Some(Ordering::Equal) | None => {
          panic!("SortedRangeFragIxs::new: overlapping Frags!")
        }
      }
    });
    res.check(fenv);
    res
  }

  pub fn unit(
    fix: RangeFragIx, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) -> Self {
    let mut res = SortedRangeFragIxs { frag_ixs: Vec::<RangeFragIx>::new() };
    res.frag_ixs.push(fix);
    res.check(fenv);
    res
  }
}

//=============================================================================
// Further methods on SortedRangeFragIxs.  These are needed by the core
// algorithm.

impl SortedRangeFragIxs {
  pub fn add(
    &mut self, to_add: &Self, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) {
    self.check(fenv);
    to_add.check(fenv);
    let sfixs_x = &self;
    let sfixs_y = &to_add;
    let mut ix = 0;
    let mut iy = 0;
    let mut res = Vec::<RangeFragIx>::new();
    while ix < sfixs_x.frag_ixs.len() && iy < sfixs_y.frag_ixs.len() {
      let fx = fenv[sfixs_x.frag_ixs[ix]];
      let fy = fenv[sfixs_y.frag_ixs[iy]];
      match cmp_range_frags(&fx, &fy) {
        Some(Ordering::Less) => {
          res.push(sfixs_x.frag_ixs[ix]);
          ix += 1;
        }
        Some(Ordering::Greater) => {
          res.push(sfixs_y.frag_ixs[iy]);
          iy += 1;
        }
        Some(Ordering::Equal) | None => {
          panic!("SortedRangeFragIxs::add: vectors intersect")
        }
      }
    }
    // At this point, one or the other or both vectors are empty.  Hence
    // it doesn't matter in which order the following two while-loops
    // appear.
    debug_assert!(ix == sfixs_x.frag_ixs.len() || iy == sfixs_y.frag_ixs.len());
    while ix < sfixs_x.frag_ixs.len() {
      res.push(sfixs_x.frag_ixs[ix]);
      ix += 1;
    }
    while iy < sfixs_y.frag_ixs.len() {
      res.push(sfixs_y.frag_ixs[iy]);
      iy += 1;
    }
    self.frag_ixs = res;
    self.check(fenv);
  }

  pub fn can_add(
    &self, to_add: &Self, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) -> bool {
    // This is merely a partial evaluation of add() which returns |false|
    // exactly in the cases where add() would have panic'd.
    self.check(fenv);
    to_add.check(fenv);
    let sfixs_x = &self;
    let sfixs_y = &to_add;
    let mut ix = 0;
    let mut iy = 0;
    while ix < sfixs_x.frag_ixs.len() && iy < sfixs_y.frag_ixs.len() {
      let fx = fenv[sfixs_x.frag_ixs[ix]];
      let fy = fenv[sfixs_y.frag_ixs[iy]];
      match cmp_range_frags(&fx, &fy) {
        Some(Ordering::Less) => {
          ix += 1;
        }
        Some(Ordering::Greater) => {
          iy += 1;
        }
        Some(Ordering::Equal) | None => {
          return false;
        }
      }
    }
    // At this point, one or the other or both vectors are empty.  So
    // we're guaranteed to succeed.
    debug_assert!(ix == sfixs_x.frag_ixs.len() || iy == sfixs_y.frag_ixs.len());
    true
  }

  pub fn del(
    &mut self, to_del: &Self, fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) {
    self.check(fenv);
    to_del.check(fenv);
    let sfixs_x = &self;
    let sfixs_y = &to_del;
    let mut ix = 0;
    let mut iy = 0;
    let mut res = Vec::<RangeFragIx>::new();
    while ix < sfixs_x.frag_ixs.len() && iy < sfixs_y.frag_ixs.len() {
      let fx = fenv[sfixs_x.frag_ixs[ix]];
      let fy = fenv[sfixs_y.frag_ixs[iy]];
      match cmp_range_frags(&fx, &fy) {
        Some(Ordering::Less) => {
          res.push(sfixs_x.frag_ixs[ix]);
          ix += 1;
        }
        Some(Ordering::Equal) => {
          ix += 1;
          iy += 1;
        }
        Some(Ordering::Greater) => {
          iy += 1;
        }
        None => panic!("SortedRangeFragIxs::del: partial overlap"),
      }
    }
    debug_assert!(ix == sfixs_x.frag_ixs.len() || iy == sfixs_y.frag_ixs.len());
    // Handle leftovers
    while ix < sfixs_x.frag_ixs.len() {
      res.push(sfixs_x.frag_ixs[ix]);
      ix += 1;
    }
    self.frag_ixs = res;
    self.check(fenv);
  }

  pub fn can_add_if_we_first_del(
    &self, to_del: &Self, to_add: &Self,
    fenv: &TypedIxVec<RangeFragIx, RangeFrag>,
  ) -> bool {
    // For now, just do this the stupid way.  It would be possible to do
    // it without any allocation, but that sounds complex.
    let mut after_del = self.clone();
    after_del.del(&to_del, fenv);
    return after_del.can_add(&to_add, fenv);
  }
}

//=============================================================================
// Representing and printing live ranges.  These are represented by two
// different but closely related types, RealRange and VirtualRange.

// RealRanges are live ranges for real regs (RealRegs).  VirtualRanges are
// live ranges for virtual regs (VirtualRegs).  VirtualRanges are the
// fundamental unit of allocation.  Both RealRange and VirtualRange pair the
// relevant kind of Reg with a vector of RangeFragIxs in which it is live.
// The RangeFragIxs are indices into some vector of RangeFrags (a "fragment
// environment", 'fenv'), which is not specified here.  They are sorted so as
// to give ascending order to the RangeFrags which they refer to.
//
// VirtualRanges contain metrics.  Not all are initially filled in:
//
// * |size| is the number of instructions in total spanned by the LR.  It must
//   not be zero.
//
// * |spill_cost| is an abstractified measure of the cost of spilling the LR.
//   The only constraint (w.r.t. correctness) is that normal LRs have a |Some|
//   value, whilst |None| is reserved for live ranges created for spills and
//   reloads and interpreted to mean "infinity".  This is needed to guarantee
//   that allocation can always succeed in the worst case, in which all of the
//   original live ranges of the program are spilled.
//
// RealRanges don't carry any metrics info since we are not trying to allocate
// them.  We merely need to work around them.
//
// I find it helpful to think of a live range, both RealRange and
// VirtualRange, as a "renaming equivalence class".  That is, if you rename
// |reg| at some point inside |sorted_frags|, then you must rename *all*
// occurrences of |reg| inside |sorted_frags|, since otherwise the program will
// no longer work.
//
// Invariants for RealRange/VirtualRange RangeFrag sets (their |sfrags| fields):
//
// * Either |sorted_frags| contains just one RangeFrag, in which case it *must*
//   be RangeFragKind::Local.
//
// * Or |sorted_frags| contains more than one RangeFrag, in which case: at
//   least one must be RangeFragKind::LiveOut, at least one must be
//   RangeFragKind::LiveIn, and there may be zero or more RangeFragKind::Thrus.

#[derive(Clone)]
pub struct RealRange {
  pub rreg: RealReg,
  pub sorted_frags: SortedRangeFragIxs,
}
impl fmt::Debug for RealRange {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    write!(fmt, "{:?} {:?}", self.rreg, self.sorted_frags)
  }
}

// VirtualRanges are live ranges for virtual regs (VirtualRegs).  These do carry
// metrics info and also the identity of the RealReg to which they eventually
// got allocated.

#[derive(Clone)]
pub struct VirtualRange {
  pub vreg: VirtualReg,
  pub rreg: Option<RealReg>,
  pub sorted_frags: SortedRangeFragIxs,
  pub size: u16,
  pub spill_cost: Option<f32>,
}

impl fmt::Debug for VirtualRange {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    let cost_str = match self.spill_cost {
      None => "INFIN".to_string(),
      Some(c) => format!("{:<5.2}", c),
    };
    write!(fmt, "{:?}", self.vreg)?;
    if self.rreg.is_some() {
      write!(fmt, " -> {:?}", self.rreg.unwrap())?;
    }
    write!(fmt, " s={}, c={} {:?}", self.size, cost_str, self.sorted_frags)
  }
}

//=============================================================================
// Test cases

#[test]
fn test_sorted_frag_ranges() {
  // Create a RangeFrag and RangeFragIx from two InstPoints.
  fn gen_fix(
    fenv: &mut TypedIxVec<RangeFragIx, RangeFrag>, first: InstPoint,
    last: InstPoint,
  ) -> RangeFragIx {
    assert!(first <= last);
    let res = RangeFragIx::new(fenv.len() as u32);
    let frag = RangeFrag {
      bix: BlockIx::new(123),
      kind: RangeFragKind::Local,
      first,
      last,
      count: 0,
    };
    fenv.push(frag);
    res
  }

  fn get_range_frag(
    fenv: &TypedIxVec<RangeFragIx, RangeFrag>, fix: RangeFragIx,
  ) -> &RangeFrag {
    &fenv[fix]
  }

  // Structural equality, at least.  Not equality in the sense of
  // deferencing the contained RangeFragIxes.
  fn sorted_range_eq(
    fixs1: &SortedRangeFragIxs, fixs2: &SortedRangeFragIxs,
  ) -> bool {
    if fixs1.frag_ixs.len() != fixs2.frag_ixs.len() {
      return false;
    }
    for (mf1, mf2) in fixs1.frag_ixs.iter().zip(&fixs2.frag_ixs) {
      if mf1 != mf2 {
        return false;
      }
    }
    true
  }

  let iix3 = InstIx::new(3);
  let iix4 = InstIx::new(4);
  let iix5 = InstIx::new(5);
  let iix6 = InstIx::new(6);
  let iix7 = InstIx::new(7);
  let iix10 = InstIx::new(10);
  let iix12 = InstIx::new(12);

  let fp_3u = InstPoint::new_use(iix3);
  let fp_3d = InstPoint::new_def(iix3);

  let fp_4u = InstPoint::new_use(iix4);

  let fp_5u = InstPoint::new_use(iix5);
  let fp_5d = InstPoint::new_def(iix5);

  let fp_6u = InstPoint::new_use(iix6);
  let fp_6d = InstPoint::new_def(iix6);

  let fp_7u = InstPoint::new_use(iix7);
  let fp_7d = InstPoint::new_def(iix7);

  let fp_10u = InstPoint::new_use(iix10);
  let fp_12u = InstPoint::new_use(iix12);

  let mut fenv = TypedIxVec::<RangeFragIx, RangeFrag>::new();

  let fix_3u = gen_fix(&mut fenv, fp_3u, fp_3u);
  let fix_3d = gen_fix(&mut fenv, fp_3d, fp_3d);
  let fix_4u = gen_fix(&mut fenv, fp_4u, fp_4u);
  let fix_3u_5u = gen_fix(&mut fenv, fp_3u, fp_5u);
  let fix_3d_5d = gen_fix(&mut fenv, fp_3d, fp_5d);
  let fix_3d_5u = gen_fix(&mut fenv, fp_3d, fp_5u);
  let fix_3u_5d = gen_fix(&mut fenv, fp_3u, fp_5d);
  let fix_6u_6d = gen_fix(&mut fenv, fp_6u, fp_6d);
  let fix_7u_7d = gen_fix(&mut fenv, fp_7u, fp_7d);
  let fix_10u = gen_fix(&mut fenv, fp_10u, fp_10u);
  let fix_12u = gen_fix(&mut fenv, fp_12u, fp_12u);

  // Boundary checks for point ranges, 3u vs 3d
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3u),
      get_range_frag(&fenv, fix_3u)
    ) == Some(Ordering::Equal)
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3u),
      get_range_frag(&fenv, fix_3d)
    ) == Some(Ordering::Less)
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3d),
      get_range_frag(&fenv, fix_3u)
    ) == Some(Ordering::Greater)
  );

  // Boundary checks for point ranges, 3d vs 4u
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3d),
      get_range_frag(&fenv, fix_3d)
    ) == Some(Ordering::Equal)
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3d),
      get_range_frag(&fenv, fix_4u)
    ) == Some(Ordering::Less)
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_4u),
      get_range_frag(&fenv, fix_3d)
    ) == Some(Ordering::Greater)
  );

  // Partially overlapping
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3d_5d),
      get_range_frag(&fenv, fix_3u_5u)
    ) == None
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3u_5u),
      get_range_frag(&fenv, fix_3d_5d)
    ) == None
  );

  // Completely overlapping: one contained within the other
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3d_5u),
      get_range_frag(&fenv, fix_3u_5d)
    ) == None
  );
  assert!(
    cmp_range_frags(
      get_range_frag(&fenv, fix_3u_5d),
      get_range_frag(&fenv, fix_3d_5u)
    ) == None
  );

  // Create a SortedRangeFragIxs from a bunch of RangeFrag indices
  fn new_sorted_frag_ranges(
    fenv: &TypedIxVec<RangeFragIx, RangeFrag>, frags: &Vec<RangeFragIx>,
  ) -> SortedRangeFragIxs {
    SortedRangeFragIxs::new(&frags, fenv)
  }

  // Construction tests
  // These fail due to overlap
  //let _ = new_sorted_frag_ranges(&fenv, &vec![fix_3u_3u, fix_3u_3u]);
  //let _ = new_sorted_frag_ranges(&fenv, &vec![fix_3u_5u, fix_3d_5d]);

  // These fail due to not being in order
  //let _ = new_sorted_frag_ranges(&fenv, &vec![fix_4u_4u, fix_3u_3u]);

  // Simple non-overlap tests for add()

  let smf_empty = new_sorted_frag_ranges(&fenv, &vec![]);
  let smf_6_7_10 =
    new_sorted_frag_ranges(&fenv, &vec![fix_6u_6d, fix_7u_7d, fix_10u]);
  let smf_3_12 = new_sorted_frag_ranges(&fenv, &vec![fix_3u, fix_12u]);
  let smf_3_6_7_10_12 = new_sorted_frag_ranges(
    &fenv,
    &vec![fix_3u, fix_6u_6d, fix_7u_7d, fix_10u, fix_12u],
  );
  let mut tmp;

  tmp = smf_empty.clone();
  tmp.add(&smf_empty, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_empty));

  tmp = smf_3_12.clone();
  tmp.add(&smf_empty, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_12));

  tmp = smf_empty.clone();
  tmp.add(&smf_3_12, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_12));

  tmp = smf_6_7_10.clone();
  tmp.add(&smf_3_12, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_6_7_10_12));

  tmp = smf_3_12.clone();
  tmp.add(&smf_6_7_10, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_6_7_10_12));

  // Tests for can_add()
  assert!(true == smf_empty.can_add(&smf_empty, &fenv));
  assert!(true == smf_empty.can_add(&smf_3_12, &fenv));
  assert!(true == smf_3_12.can_add(&smf_empty, &fenv));
  assert!(false == smf_3_12.can_add(&smf_3_12, &fenv));

  assert!(true == smf_6_7_10.can_add(&smf_3_12, &fenv));

  assert!(true == smf_3_12.can_add(&smf_6_7_10, &fenv));

  // Tests for del()
  let smf_6_7 = new_sorted_frag_ranges(&fenv, &vec![fix_6u_6d, fix_7u_7d]);
  let smf_6_10 = new_sorted_frag_ranges(&fenv, &vec![fix_6u_6d, fix_10u]);
  let smf_7 = new_sorted_frag_ranges(&fenv, &vec![fix_7u_7d]);
  let smf_10 = new_sorted_frag_ranges(&fenv, &vec![fix_10u]);

  tmp = smf_empty.clone();
  tmp.del(&smf_empty, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_empty));

  tmp = smf_3_12.clone();
  tmp.del(&smf_empty, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_12));

  tmp = smf_empty.clone();
  tmp.del(&smf_3_12, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_empty));

  tmp = smf_6_7_10.clone();
  tmp.del(&smf_3_12, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_6_7_10));

  tmp = smf_3_12.clone();
  tmp.del(&smf_6_7_10, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_3_12));

  tmp = smf_6_7_10.clone();
  tmp.del(&smf_6_7, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_10));

  tmp = smf_6_7_10.clone();
  tmp.del(&smf_10, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_6_7));

  tmp = smf_6_7_10.clone();
  tmp.del(&smf_7, &fenv);
  assert!(sorted_range_eq(&tmp, &smf_6_10));

  // Tests for can_add_if_we_first_del()
  let smf_10_12 = new_sorted_frag_ranges(&fenv, &vec![fix_10u, fix_12u]);

  assert!(
    true
      == smf_6_7_10
        .can_add_if_we_first_del(/*d=*/ &smf_10_12, /*a=*/ &smf_3_12, &fenv)
  );

  assert!(
    false
      == smf_6_7_10
        .can_add_if_we_first_del(/*d=*/ &smf_10_12, /*a=*/ &smf_7, &fenv)
  );
}
