//! Support types for generated encoding tables.
//!
//! This module contains types and functions for working with the encoding tables generated by
//! `lib/cretonne/meta/gen_encoding.py`.
use ir::{Type, Opcode};
use isa::{Encoding, Legalize};
use constant_hash::{Table, probe};

/// Level 1 hash table entry.
///
/// One level 1 hash table is generated per CPU mode. This table is keyed by the controlling type
/// variable, using `VOID` for non-polymorphic instructions.
///
/// The hash table values are references to level 2 hash tables, encoded as an offset in `LEVEL2`
/// where the table begins, and the binary logarithm of its length. All the level 2 hash tables
/// have a power-of-two size.
///
/// Entries are generic over the offset type. It will typically be `u32` or `u16`, depending on the
/// size of the `LEVEL2` table. A `u16` offset allows entries to shrink to 32 bits each, but some
/// ISAs may have tables so large that `u32` offsets are needed.
///
/// Empty entries are encoded with a 0 `log2len`. This is on the assumption that no level 2 tables
/// have only a single entry.
pub struct Level1Entry<OffT: Into<u32> + Copy> {
    pub ty: Type,
    pub log2len: u8,
    pub offset: OffT,
}

impl<OffT: Into<u32> + Copy> Table<Type> for [Level1Entry<OffT>] {
    fn len(&self) -> usize {
        self.len()
    }

    fn key(&self, idx: usize) -> Option<Type> {
        if self[idx].log2len != 0 {
            Some(self[idx].ty)
        } else {
            None
        }
    }
}

/// Level 2 hash table entry.
///
/// The second level hash tables are keyed by `Opcode`, and contain an offset into the `ENCLISTS`
/// table where the encoding recipes for the instrution are stored.
///
/// Entries are generic over the offset type which depends on the size of `ENCLISTS`. A `u16`
/// offset allows the entries to be only 32 bits each. There is no benefit to dropping down to `u8`
/// for tiny ISAs. The entries won't shrink below 32 bits since the opcode is expected to be 16
/// bits.
///
/// Empty entries are encoded with a `NotAnOpcode` `opcode` field.
pub struct Level2Entry<OffT: Into<u32> + Copy> {
    pub opcode: Option<Opcode>,
    pub offset: OffT,
}

impl<OffT: Into<u32> + Copy> Table<Opcode> for [Level2Entry<OffT>] {
    fn len(&self) -> usize {
        self.len()
    }

    fn key(&self, idx: usize) -> Option<Opcode> {
        self[idx].opcode
    }
}

/// Two-level hash table lookup.
///
/// Given the controlling type variable and instruction opcode, find the corresponding encoding
/// list.
///
/// Returns an offset into the ISA's `ENCLIST` table, or `None` if the opcode/type combination is
/// not legal.
pub fn lookup_enclist<OffT1, OffT2>(ctrl_typevar: Type,
                                    opcode: Opcode,
                                    level1_table: &[Level1Entry<OffT1>],
                                    level2_table: &[Level2Entry<OffT2>])
                                    -> Result<usize, Legalize>
    where OffT1: Into<u32> + Copy,
          OffT2: Into<u32> + Copy
{
    // TODO: The choice of legalization actions here is naive. This needs to be configurable.
    probe(level1_table, ctrl_typevar, ctrl_typevar.index())
        .ok_or_else(|| if ctrl_typevar.lane_type().bits() > 32 {
                        Legalize::Narrow
                    } else {
                        Legalize::Expand
                    })
        .and_then(|l1idx| {
            let l1ent = &level1_table[l1idx];
            let l2off = l1ent.offset.into() as usize;
            let l2tab = &level2_table[l2off..l2off + (1 << l1ent.log2len)];
            probe(l2tab, opcode, opcode as usize)
                .map(|l2idx| l2tab[l2idx].offset.into() as usize)
                .ok_or(Legalize::Expand)
        })
}

/// Encoding list entry.
///
/// Encoding lists are represented as sequences of u16 words.
pub type EncListEntry = u16;

/// Number of bits used to represent a predicate. c.f. `meta.gen_encoding.py`.
const PRED_BITS: u8 = 12;
const PRED_MASK: EncListEntry = (1 << PRED_BITS) - 1;

/// The match-always instruction predicate. c.f. `meta.gen_encoding.py`.
const CODE_ALWAYS: EncListEntry = PRED_MASK;

/// The encoding list terminator.
const CODE_FAIL: EncListEntry = 0xffff;

/// Find the most general encoding of `inst`.
///
/// Given an encoding list offset as returned by `lookup_enclist` above, search the encoding list
/// for the most general encoding that applies to `inst`. The encoding lists are laid out such that
/// this is the last valid entry in the list.
///
/// This function takes two closures that are used to evaluate predicates:
/// - `instp` is passed an instruction predicate number to be evaluated on the current instruction.
/// - `isap` is passed an ISA predicate number to evaluate.
///
/// Returns the corresponding encoding, or `None` if no list entries are satisfied by `inst`.
pub fn general_encoding<InstP, IsaP>(offset: usize,
                                     enclist: &[EncListEntry],
                                     instp: InstP,
                                     isap: IsaP)
                                     -> Option<Encoding>
    where InstP: Fn(EncListEntry) -> bool,
          IsaP: Fn(EncListEntry) -> bool
{
    let mut found = None;
    let mut pos = offset;
    while enclist[pos] != CODE_FAIL {
        let pred = enclist[pos];
        if pred <= CODE_ALWAYS {
            // This is an instruction predicate followed by recipe and encbits entries.
            if pred == CODE_ALWAYS || instp(pred) {
                found = Some(Encoding::new(enclist[pos + 1], enclist[pos + 2]))
            }
            pos += 3;
        } else {
            // This is an ISA predicate entry.
            pos += 1;
            if !isap(pred & PRED_MASK) {
                // ISA predicate failed, skip the next N entries.
                pos += 3 * (pred >> PRED_BITS) as usize;
            }
        }
    }
    found
}
