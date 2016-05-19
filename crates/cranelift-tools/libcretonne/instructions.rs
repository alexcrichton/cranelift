//! Instruction formats and opcodes.
//!
//! The `instructions` module contains definitions for instruction formats, opcodes, and the
//! in-memory representation of IL instructions.
//!
//! A large part of this module is auto-generated from the instruction descriptions in the meta
//! directory.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use entities::*;
use immediates::*;
use types::Type;

// Include code generated by `meta/gen_instr.py`. This file contains:
//
// - The `pub enum InstructionFormat` enum with all the instruction formats.
// - The `pub enum Opcode` definition with all known opcodes,
// - The `const OPCODE_FORMAT: [InstructionFormat; N]` table.
// - The private `fn opcode_name(Opcode) -> &'static str` function, and
// - The hash table `const OPCODE_HASH_TABLE: [Opcode; N]`.
//
include!(concat!(env!("OUT_DIR"), "/opcodes.rs"));

impl Display for Opcode {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", opcode_name(*self))
    }
}

impl Opcode {
    /// Get the instruction format for this opcode.
    pub fn format(self) -> Option<InstructionFormat> {
        if self == Opcode::NotAnOpcode {
            None
        } else {
            Some(OPCODE_FORMAT[self as usize - 1])
        }
    }
}

// A primitive hash function for matching opcodes.
// Must match `meta/constant_hash.py`.
fn simple_hash(s: &str) -> u32 {
    let mut h: u32 = 5381;
    for c in s.chars() {
        h = (h ^ c as u32).wrapping_add(h.rotate_right(6));
    }
    h
}

// This trait really belongs in libreader where it is used by the .cton file parser, but since it
// critically depends on the `opcode_name()` function which is needed here anyway, it lives in this
// module. This also saves us from runing the build script twice to generate code for the two
// separate crates.
impl FromStr for Opcode {
    type Err = &'static str;

    /// Parse an Opcode name from a string.
    fn from_str(s: &str) -> Result<Opcode, &'static str> {
        let tlen = OPCODE_HASH_TABLE.len();
        assert!(tlen.is_power_of_two());
        let mut idx = simple_hash(s) as usize;
        let mut step: usize = 0;
        loop {
            idx = idx % tlen;
            let entry = OPCODE_HASH_TABLE[idx];

            if entry == Opcode::NotAnOpcode {
                return Err("Unknown opcode");
            }

            if *opcode_name(entry) == *s {
                return Ok(entry);
            }

            // Quadratic probing.
            step += 1;
            // When `tlen` is a power of two, it can be proven that idx will visit all entries.
            // This means that this loop will always terminate if the hash table has even one
            // unused entry.
            assert!(step < tlen);
            idx += step;
        }
    }
}

/// Contents on an instruction.
///
/// Every variant must contain `opcode` and `ty` fields. An instruction that doesn't produce a
/// value should have its `ty` field set to `VOID`. The size of `InstructionData` should be kept at
/// 16 bytes on 64-bit architectures. If more space is needed to represent an instruction, use a
/// `Box<AuxData>` to store the additional information out of line.
#[derive(Debug)]
pub enum InstructionData {
    Nullary {
        opcode: Opcode,
        ty: Type,
    },
    Unary {
        opcode: Opcode,
        ty: Type,
        arg: Value,
    },
    UnaryImm {
        opcode: Opcode,
        ty: Type,
        imm: Imm64,
    },
    UnaryIeee32 {
        opcode: Opcode,
        ty: Type,
        imm: Ieee32,
    },
    UnaryIeee64 {
        opcode: Opcode,
        ty: Type,
        imm: Ieee64,
    },
    UnaryImmVector {
        opcode: Opcode,
        ty: Type, // TBD: imm: Box<ImmVectorData>
    },
    Binary {
        opcode: Opcode,
        ty: Type,
        args: [Value; 2],
    },
    BinaryImm {
        opcode: Opcode,
        ty: Type,
        lhs: Value,
        rhs: Imm64,
    },
    // Same as BinaryImm, but the imediate is the lhs operand.
    BinaryImmRev {
        opcode: Opcode,
        ty: Type,
        rhs: Value,
        lhs: Imm64,
    },
    Jump {
        opcode: Opcode,
        ty: Type,
        data: Box<JumpData>,
    },
    Branch {
        opcode: Opcode,
        ty: Type,
        data: Box<BranchData>,
    },
    BranchTable {
        opcode: Opcode,
        ty: Type,
        arg: Value,
        table: JumpTable,
    },
    Call {
        opcode: Opcode,
        ty: Type,
        data: Box<CallData>,
    },
}

/// A variable list of `Value` operands used for function call arguments and passing arguments to
/// basic blocks.
#[derive(Debug)]
pub struct VariableArgs(Vec<Value>);

impl VariableArgs {
    pub fn new() -> VariableArgs {
        VariableArgs(Vec::new())
    }
}

impl Display for VariableArgs {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        try!(write!(fmt, "("));
        for (i, val) in self.0.iter().enumerate() {
            if i == 0 {
                try!(write!(fmt, "{}", val));
            } else {
                try!(write!(fmt, ", {}", val));
            }
        }
        write!(fmt, ")")
    }
}

impl Default for VariableArgs {
    fn default() -> VariableArgs {
        VariableArgs::new()
    }
}

/// Payload data for jump instructions. These need to carry lists of EBB arguments that won't fit
/// in the allowed InstructionData size.
#[derive(Debug)]
pub struct JumpData {
    destination: Ebb,
    arguments: VariableArgs,
}

impl Display for JumpData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}{}", self.destination, self.arguments)
    }
}

/// Payload data for branch instructions. These need to carry lists of EBB arguments that won't fit
/// in the allowed InstructionData size.
#[derive(Debug)]
pub struct BranchData {
    arg: Value,
    destination: Ebb,
    arguments: VariableArgs,
}

impl Display for BranchData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}, {}{}", self.arg, self.destination, self.arguments)
    }
}

/// Payload of a call instruction.
#[derive(Debug)]
pub struct CallData {
    /// Second result value for a call producing multiple return values.
    second_result: Value,

    // Dynamically sized array containing call argument values.
    arguments: VariableArgs,
}

impl Display for CallData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "TBD{}", self.arguments)
    }
}

impl InstructionData {
    /// Create data for a call instruction.
    pub fn call(opc: Opcode, return_type: Type) -> InstructionData {
        InstructionData::Call {
            opcode: opc,
            ty: return_type,
            data: Box::new(CallData {
                second_result: NO_VALUE,
                arguments: VariableArgs::new(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes() {
        let x = Opcode::Iadd;
        let mut y = Opcode::Isub;

        assert!(x != y);
        y = Opcode::Iadd;
        assert_eq!(x, y);
        assert_eq!(x.format(), Some(InstructionFormat::Binary));

        assert_eq!(format!("{:?}", Opcode::IaddImm), "IaddImm");
        assert_eq!(Opcode::IaddImm.to_string(), "iadd_imm");

        // Check the matcher.
        assert_eq!("iadd".parse::<Opcode>(), Ok(Opcode::Iadd));
        assert_eq!("iadd_imm".parse::<Opcode>(), Ok(Opcode::IaddImm));
        assert_eq!("iadd\0".parse::<Opcode>(), Err("Unknown opcode"));
        assert_eq!("".parse::<Opcode>(), Err("Unknown opcode"));
        assert_eq!("\0".parse::<Opcode>(), Err("Unknown opcode"));
    }

    #[test]
    fn instruction_data() {
        use std::mem;
        // The size of the InstructionData enum is important for performance. It should not exceed
        // 16 bytes. Use `Box<FooData>` out-of-line payloads for instruction formats that require
        // more space than that.
        // It would be fine with a data structure smaller than 16 bytes, but what are the odds of
        // that?
        assert_eq!(mem::size_of::<InstructionData>(), 16);
    }
}
