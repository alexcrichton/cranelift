use runtime::{FuncEnvironment, GlobalValue, WasmRuntime};
use translation_utils::{Global, Memory, Table, GlobalIndex, TableIndex, SignatureIndex,
                        FunctionIndex, MemoryIndex};
use cretonne::ir::{self, InstBuilder};
use cretonne::ir::types::*;
use cretonne::cursor::FuncCursor;
use cretonne::settings;

/// This runtime implementation is a "naïve" one, doing essentially nothing and emitting
/// placeholders when forced to. Don't try to execute code translated with this runtime, it is
/// essentially here for translation debug purposes.
pub struct DummyRuntime {
    // Unprocessed signatures exactly as provided by `declare_signature()`.
    signatures: Vec<ir::Signature>,
    globals: Vec<Global>,

    // Types of functions, imported and local.
    func_types: Vec<SignatureIndex>,

    // Names of imported functions.
    imported_funcs: Vec<ir::FunctionName>,

    // Compilation setting flags.
    flags: settings::Flags,

    // The start function.
    start_func: Option<FunctionIndex>,
}

impl DummyRuntime {
    /// Allocates the runtime data structures with default flags.
    pub fn default() -> Self {
        Self::with_flags(settings::Flags::new(&settings::builder()))
    }

    /// Allocates the runtime data structures with the given flags.
    pub fn with_flags(flags: settings::Flags) -> Self {
        Self {
            signatures: Vec::new(),
            globals: Vec::new(),
            func_types: Vec::new(),
            imported_funcs: Vec::new(),
            flags,
            start_func: None,
        }
    }
}

impl FuncEnvironment for DummyRuntime {
    fn flags(&self) -> &settings::Flags {
        &self.flags
    }

    fn make_global(&mut self, func: &mut ir::Function, index: GlobalIndex) -> GlobalValue {
        // Just create a dummy `vmctx` global.
        let offset = ((index * 8) as i32 + 8).into();
        let gv = func.create_global_var(ir::GlobalVarData::VmCtx { offset });
        GlobalValue::Memory {
            gv,
            ty: self.globals[index].ty,
        }
    }

    fn make_heap(&mut self, func: &mut ir::Function, _index: MemoryIndex) -> ir::Heap {
        func.create_heap(ir::HeapData {
            base: ir::HeapBase::ReservedReg,
            min_size: 0.into(),
            guard_size: 0x8000_0000.into(),
            style: ir::HeapStyle::Static { bound: 0x1_0000_0000.into() },
        })
    }

    fn make_indirect_sig(&mut self, func: &mut ir::Function, index: SignatureIndex) -> ir::SigRef {
        // A real implementation would probably change the calling convention and add `vmctx` and
        // signature index arguments.
        func.import_signature(self.signatures[index].clone())
    }

    fn make_direct_func(&mut self, func: &mut ir::Function, index: FunctionIndex) -> ir::FuncRef {
        let sigidx = self.func_types[index];
        // A real implementation would probably add a `vmctx` argument.
        // And maybe attempt some signature de-duplication.
        let signature = func.import_signature(self.signatures[sigidx].clone());

        let name = match self.imported_funcs.get(index) {
            Some(name) => name.clone(),
            None => ir::FunctionName::new(format!("localfunc{}", index)),
        };

        func.import_function(ir::ExtFuncData { name, signature })
    }

    fn translate_call_indirect(
        &mut self,
        mut pos: FuncCursor,
        _table_index: TableIndex,
        _sig_index: SignatureIndex,
        sig_ref: ir::SigRef,
        callee: ir::Value,
        call_args: &[ir::Value],
    ) -> ir::Inst {
        pos.ins().call_indirect(sig_ref, callee, call_args)
    }

    fn translate_grow_memory(
        &mut self,
        mut pos: FuncCursor,
        _index: MemoryIndex,
        _heap: ir::Heap,
        _val: ir::Value,
    ) -> ir::Value {
        pos.ins().iconst(I32, -1)
    }

    fn translate_current_memory(
        &mut self,
        mut pos: FuncCursor,
        _index: MemoryIndex,
        _heap: ir::Heap,
    ) -> ir::Value {
        pos.ins().iconst(I32, -1)
    }
}

impl WasmRuntime for DummyRuntime {
    fn declare_signature(&mut self, sig: &ir::Signature) {
        self.signatures.push(sig.clone());
    }

    fn get_signature(&self, sig_index: SignatureIndex) -> &ir::Signature {
        &self.signatures[sig_index]
    }

    fn declare_func_import(&mut self, sig_index: SignatureIndex, module: &[u8], field: &[u8]) {
        assert_eq!(
            self.func_types.len(),
            self.imported_funcs.len(),
            "Imported functions must be declared first"
        );
        self.func_types.push(sig_index);

        let mut name = Vec::new();
        name.extend(module.iter().cloned().map(name_fold));
        name.push(b'_');
        name.extend(field.iter().cloned().map(name_fold));
        self.imported_funcs.push(ir::FunctionName::new(name));
    }

    fn get_num_func_imports(&self) -> usize {
        self.imported_funcs.len()
    }

    fn declare_func_type(&mut self, sig_index: SignatureIndex) {
        self.func_types.push(sig_index);
    }

    fn get_func_type(&self, func_index: FunctionIndex) -> SignatureIndex {
        self.func_types[func_index]
    }

    fn declare_global(&mut self, global: Global) {
        self.globals.push(global);
    }

    fn get_global(&self, global_index: GlobalIndex) -> &Global {
        &self.globals[global_index]
    }

    fn declare_table(&mut self, _: Table) {
        //We do nothing
    }
    fn declare_table_elements(&mut self, _: TableIndex, _: usize, _: &[FunctionIndex]) {
        //We do nothing
    }
    fn declare_memory(&mut self, _: Memory) {
        //We do nothing
    }
    fn declare_data_initialization(
        &mut self,
        _: MemoryIndex,
        _: usize,
        _: &[u8],
    ) -> Result<(), String> {
        // We do nothing
        Ok(())
    }

    fn declare_start_func(&mut self, func_index: FunctionIndex) {
        debug_assert!(self.start_func.is_none());
        self.start_func = Some(func_index);
    }

    fn begin_translation(&mut self) {
        // We do nothing
    }
    fn next_function(&mut self) {
        // We do nothing
    }
}

// Generate characters suitable for printable `FuncName`s.
fn name_fold(c: u8) -> u8 {
    if (c as char).is_alphanumeric() {
        c
    } else {
        b'_'
    }
}
