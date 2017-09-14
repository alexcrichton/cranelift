//! A simple GVN pass.

use flowgraph::ControlFlowGraph;
use dominator_tree::DominatorTree;
use ir::{Cursor, CursorBase, InstructionData, Function, Inst, Opcode, Type};
use std::collections::HashMap;

/// Test whether the given opcode is unsafe to even consider for GVN.
fn trivially_unsafe_for_gvn(opcode: Opcode) -> bool {
    opcode.is_call() || opcode.is_branch() || opcode.is_terminator() || opcode.is_return() ||
        opcode.can_trap() || opcode.other_side_effects()
}

/// Perform simple GVN on `func`.
///
pub fn do_simple_gvn(func: &mut Function, cfg: &mut ControlFlowGraph, domtree: &mut DominatorTree) {
    debug_assert!(cfg.is_valid());
    debug_assert!(domtree.is_valid());

    let mut visible_values: HashMap<(InstructionData, Type), Inst> = HashMap::new();

    // Visit EBBs in a reverse post-order.
    let mut pos = Cursor::new(&mut func.layout);

    for &ebb in domtree.cfg_postorder().iter().rev() {
        pos.goto_top(ebb);

        while let Some(inst) = pos.next_inst() {
            let opcode = func.dfg[inst].opcode();
            let ctrl_typevar = func.dfg.ctrl_typevar(inst);

            // Resolve aliases, particularly aliases we created earlier.
            func.dfg.resolve_aliases_in_arguments(inst);

            if trivially_unsafe_for_gvn(opcode) {
                continue;
            }

            // TODO: Implement simple redundant-load elimination.
            if opcode.can_store() {
                continue;
            }
            if opcode.can_load() {
                continue;
            }

            let key = (func.dfg[inst].clone(), ctrl_typevar);
            let entry = visible_values.entry(key);
            use std::collections::hash_map::Entry::*;
            match entry {
                Occupied(mut entry) => {
                    if domtree.dominates(*entry.get(), inst, pos.layout) {
                        func.dfg.replace_with_aliases(inst, *entry.get());
                        pos.remove_inst_and_step_back();
                    } else {
                        // The prior instruction doesn't dominate inst, so it
                        // won't dominate any subsequent instructions we'll
                        // visit, so just replace it.
                        *entry.get_mut() = inst;
                        continue;
                    }
                }
                Vacant(entry) => {
                    entry.insert(inst);
                }
            }
        }
    }
}
