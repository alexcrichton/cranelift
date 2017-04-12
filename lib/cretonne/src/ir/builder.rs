//! Cretonne instruction builder.
//!
//! A `Builder` provides a convenient interface for inserting instructions into a Cretonne
//! function. Many of its methods are generated from the meta language instruction definitions.

use ir::types;
use ir::{InstructionData, DataFlowGraph, Cursor};
use ir::{Opcode, Type, Inst, Value, Ebb, JumpTable, SigRef, FuncRef, StackSlot, ValueList,
         MemFlags};
use ir::immediates::{Imm64, Uimm8, Ieee32, Ieee64, Offset32, Uoffset32};
use ir::condcodes::{IntCC, FloatCC};

/// Base trait for instruction builders.
///
/// The `InstBuilderBase` trait provides the basic functionality required by the methods of the
/// generated `InstBuilder` trait. These methods should not normally be used directly. Use the
/// methods in the `InstBuilder` trait instead.
///
/// Any data type that implements `InstBuilderBase` also gets all the methods of the `InstBuilder`
/// trait.
pub trait InstBuilderBase<'f>: Sized {
    /// Get an immutable reference to the data flow graph that will hold the constructed
    /// instructions.
    fn data_flow_graph(&self) -> &DataFlowGraph;
    fn data_flow_graph_mut(&mut self) -> &mut DataFlowGraph;

    /// Insert an instruction and return a reference to it, consuming the builder.
    ///
    /// The result types may depend on a controlling type variable. For non-polymorphic
    /// instructions with multiple results, pass `VOID` for the `ctrl_typevar` argument.
    fn build(self, data: InstructionData, ctrl_typevar: Type) -> (Inst, &'f mut DataFlowGraph);
}

// Include trait code generated by `lib/cretonne/meta/gen_instr.py`.
//
// This file defines the `InstBuilder` trait as an extension of `InstBuilderBase` with methods per
// instruction format and per opcode.
include!(concat!(env!("OUT_DIR"), "/builder.rs"));

/// Any type implementing `InstBuilderBase` gets all the `InstBuilder` methods for free.
impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}

/// Builder that inserts an instruction at the current cursor position.
///
/// An `InsertBuilder` holds mutable references to a data flow graph and a layout cursor. It
/// provides convenience methods for creating and inserting instructions at the current cursor
/// position.
pub struct InsertBuilder<'c, 'fc: 'c, 'fd> {
    pos: &'c mut Cursor<'fc>,
    dfg: &'fd mut DataFlowGraph,
}

impl<'c, 'fc, 'fd> InsertBuilder<'c, 'fc, 'fd> {
    /// Create a new builder which inserts instructions at `pos`.
    /// The `dfg` and `pos.layout` references should be from the same `Function`.
    pub fn new(dfg: &'fd mut DataFlowGraph,
               pos: &'c mut Cursor<'fc>)
               -> InsertBuilder<'c, 'fc, 'fd> {
        InsertBuilder { dfg: dfg, pos: pos }
    }
}

impl<'c, 'fc, 'fd> InstBuilderBase<'fd> for InsertBuilder<'c, 'fc, 'fd> {
    fn data_flow_graph(&self) -> &DataFlowGraph {
        self.dfg
    }

    fn data_flow_graph_mut(&mut self) -> &mut DataFlowGraph {
        self.dfg
    }

    fn build(self, data: InstructionData, ctrl_typevar: Type) -> (Inst, &'fd mut DataFlowGraph) {
        let inst = self.dfg.make_inst(data);
        self.dfg.make_inst_results(inst, ctrl_typevar);
        self.pos.insert_inst(inst);
        (inst, self.dfg)
    }
}

/// Instruction builder that replaces an existing instruction.
///
/// The inserted instruction will have the same `Inst` number as the old one.
///
/// If the old instruction still has result values attached, it is assumed that the new instruction
/// produces the same number and types of results. The old result values are preserved. If the
/// replacement instruction format does not support multiple results, the builder panics. It is a
/// bug to leave result values dangling.
pub struct ReplaceBuilder<'f> {
    dfg: &'f mut DataFlowGraph,
    inst: Inst,
}

impl<'f> ReplaceBuilder<'f> {
    /// Create a `ReplaceBuilder` that will overwrite `inst`.
    pub fn new(dfg: &'f mut DataFlowGraph, inst: Inst) -> ReplaceBuilder {
        ReplaceBuilder {
            dfg: dfg,
            inst: inst,
        }
    }
}

impl<'f> InstBuilderBase<'f> for ReplaceBuilder<'f> {
    fn data_flow_graph(&self) -> &DataFlowGraph {
        self.dfg
    }

    fn data_flow_graph_mut(&mut self) -> &mut DataFlowGraph {
        self.dfg
    }

    fn build(self, data: InstructionData, ctrl_typevar: Type) -> (Inst, &'f mut DataFlowGraph) {
        // Splat the new instruction on top of the old one.
        self.dfg[self.inst] = data;

        if !self.dfg.has_results(self.inst) {
            // The old result values were either detached or non-existent.
            // Construct new ones.
            self.dfg.make_inst_results(self.inst, ctrl_typevar);
        } else {
            // Reattach the old secondary values.
            let old_second_value = self.dfg.inst_results(self.inst).get(1).cloned();
            if let Some(val_ref) = self.dfg[self.inst].second_result_mut() {
                // Don't check types here. Leave that to the verifier.
                *val_ref = old_second_value.into();
            }

            // Normally, make_inst_results() would also set the first result type, but we're not
            // going to call that, so set it manually.
            *self.dfg[self.inst].first_type_mut() = self.dfg
                .compute_result_type(self.inst, 0, ctrl_typevar)
                .unwrap_or_default();
        }

        (self.inst, self.dfg)
    }
}

#[cfg(test)]
mod tests {
    use ir::{Function, Cursor, InstBuilder};
    use ir::types::*;
    use ir::condcodes::*;

    #[test]
    fn types() {
        let mut func = Function::new();
        let dfg = &mut func.dfg;
        let ebb0 = dfg.make_ebb();
        let arg0 = dfg.append_ebb_arg(ebb0, I32);
        let pos = &mut Cursor::new(&mut func.layout);
        pos.insert_ebb(ebb0);

        // Explicit types.
        let v0 = dfg.ins(pos).iconst(I32, 3);
        assert_eq!(dfg.value_type(v0), I32);

        // Inferred from inputs.
        let v1 = dfg.ins(pos).iadd(arg0, v0);
        assert_eq!(dfg.value_type(v1), I32);

        // Formula.
        let cmp = dfg.ins(pos).icmp(IntCC::Equal, arg0, v0);
        assert_eq!(dfg.value_type(cmp), B1);
    }
}
