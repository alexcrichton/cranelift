//! Code sink that writes binary machine code into contiguous memory.
//!
//! The `CodeSink` trait is the most general way of extracting binary machine code from Cranelift,
//! and it is implemented by things like the `test binemit` file test driver to generate
//! hexadecimal machine code. The `CodeSink` has some undesirable performance properties because of
//! the dual abstraction: `TargetIsa` is a trait object implemented by each supported ISA, so it
//! can't have any generic functions that could be specialized for each `CodeSink` implementation.
//! This results in many virtual function callbacks (one per `put*` call) when
//! `TargetIsa::emit_inst()` is used.
//!
//! The `MemoryCodeSink` type fixes the performance problem because it is a type known to
//! `TargetIsa` so it can specialize its machine code generation for the type. The trade-off is
//! that a `MemoryCodeSink` will always write binary machine code to raw memory. It forwards any
//! relocations to a `RelocSink` trait object. Relocations are less frequent than the
//! `CodeSink::put*` methods, so the performance impact of the virtual callbacks is less severe.

use super::{Addend, CodeInfo, CodeOffset, CodeSink, Reloc};
use crate::ir::{ExternalName, JumpTable, SourceLoc, TrapCode};
use core::ptr::write_unaligned;

/// A `CodeSink` that writes binary machine code directly into memory.
///
/// A `MemoryCodeSink` object should be used when emitting a Cranelift IR function into executable
/// memory. It writes machine code directly to a raw pointer without any bounds checking, so make
/// sure to allocate enough memory for the whole function. The number of bytes required is returned
/// by the `Context::compile()` function.
///
/// Any relocations in the function are forwarded to the `RelocSink` trait object.
///
/// Note that `MemoryCodeSink` writes multi-byte values in the native byte order of the host. This
/// is not the right thing to do for cross compilation.
pub struct MemoryCodeSink<'a> {
    /// Pointer to start of sink's preallocated memory.
    data: *mut u8,
    /// Offset is isize because its major consumer needs it in that form.
    offset: isize,
    relocs: &'a mut RelocSink,
    traps: &'a mut TrapSink,
    /// Information about the generated code and read-only data.
    pub info: CodeInfo,
}

impl<'a> MemoryCodeSink<'a> {
    /// Create a new memory code sink that writes a function to the memory pointed to by `data`.
    ///
    /// This function is unsafe since `MemoryCodeSink` does not perform bounds checking on the
    /// memory buffer, and it can't guarantee that the `data` pointer is valid.
    pub unsafe fn new(data: *mut u8, relocs: &'a mut RelocSink, traps: &'a mut TrapSink) -> Self {
        Self {
            data,
            offset: 0,
            info: CodeInfo {
                code_size: 0,
                jumptables_size: 0,
                rodata_size: 0,
                total_size: 0,
            },
            relocs,
            traps,
        }
    }
}

/// A trait for receiving relocations for code that is emitted directly into memory.
pub trait RelocSink {
    /// Add a relocation referencing an EBB at the current offset.
    fn reloc_ebb(&mut self, _: CodeOffset, _: Reloc, _: CodeOffset);

    /// Add a relocation referencing an external symbol at the current offset.
    fn reloc_external(&mut self, _: CodeOffset, _: Reloc, _: &ExternalName, _: Addend);

    /// Add a relocation referencing a jump table.
    fn reloc_jt(&mut self, _: CodeOffset, _: Reloc, _: JumpTable);
}

/// A trait for receiving trap codes and offsets.
///
/// If you don't need information about possible traps, you can use the
/// [`NullTrapSink`](binemit/trait.TrapSink.html) implementation.
pub trait TrapSink {
    /// Add trap information for a specific offset.
    fn trap(&mut self, _: CodeOffset, _: SourceLoc, _: TrapCode);
}

impl<'a> MemoryCodeSink<'a> {
    fn write<T>(&mut self, x: T) {
        unsafe {
            #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_ptr_alignment))]
            write_unaligned(self.data.offset(self.offset) as *mut T, x);
            self.offset += std::mem::size_of::<T>() as isize;
        }
    }
}

impl<'a> CodeSink for MemoryCodeSink<'a> {
    fn offset(&self) -> CodeOffset {
        self.offset as CodeOffset
    }

    fn put1(&mut self, x: u8) {
        self.write(x);
    }

    fn put2(&mut self, x: u16) {
        self.write(x);
    }

    fn put4(&mut self, x: u32) {
        self.write(x);
    }

    fn put8(&mut self, x: u64) {
        self.write(x);
    }

    fn reloc_ebb(&mut self, rel: Reloc, ebb_offset: CodeOffset) {
        let ofs = self.offset();
        self.relocs.reloc_ebb(ofs, rel, ebb_offset);
    }

    fn reloc_external(&mut self, rel: Reloc, name: &ExternalName, addend: Addend) {
        let ofs = self.offset();
        self.relocs.reloc_external(ofs, rel, name, addend);
    }

    fn reloc_jt(&mut self, rel: Reloc, jt: JumpTable) {
        let ofs = self.offset();
        self.relocs.reloc_jt(ofs, rel, jt);
    }

    fn trap(&mut self, code: TrapCode, srcloc: SourceLoc) {
        let ofs = self.offset();
        self.traps.trap(ofs, srcloc, code);
    }

    fn begin_jumptables(&mut self) {
        self.info.code_size = self.offset();
    }

    fn begin_rodata(&mut self) {
        self.info.jumptables_size = self.offset() - self.info.code_size;
    }

    fn end_codegen(&mut self) {
        self.info.rodata_size = self.offset() - self.info.jumptables_size;
        self.info.total_size = self.offset();
    }
}

/// A `TrapSink` implementation that does nothing, which is convenient when
/// compiling code that does not rely on trapping semantics.
pub struct NullTrapSink {}

impl TrapSink for NullTrapSink {
    fn trap(&mut self, _offset: CodeOffset, _srcloc: SourceLoc, _code: TrapCode) {}
}
