//! Defines `Module` and related types.

// TODO: Should `ir::Function` really have a `name`?

// TODO: Factor out `ir::Function`'s `ext_funcs` and `global_values` into a struct
// shared with `DataContext`?

use cranelift_codegen::entity::{EntityRef, PrimaryMap};
use cranelift_codegen::{binemit, ir, CodegenError, Context};
use data_context::DataContext;
use std::borrow::ToOwned;
use std::collections::HashMap;
use std::string::String;
use Backend;

/// A function identifier for use in the `Module` interface.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(u32);
entity_impl!(FuncId, "funcid");

/// Function identifiers are namespace 0 in `ir::ExternalName`
impl From<FuncId> for ir::ExternalName {
    fn from(id: FuncId) -> Self {
        ir::ExternalName::User {
            namespace: 0,
            index: id.0,
        }
    }
}

/// A data object identifier for use in the `Module` interface.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataId(u32);
entity_impl!(DataId, "dataid");

/// Data identifiers are namespace 1 in `ir::ExternalName`
impl From<DataId> for ir::ExternalName {
    fn from(id: DataId) -> Self {
        ir::ExternalName::User {
            namespace: 1,
            index: id.0,
        }
    }
}

/// Linkage refers to where an entity is defined and who can see it.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Linkage {
    /// Defined outside of a module.
    Import,
    /// Defined inside the module, but not visible outside it.
    Local,
    /// Defined inside the module, visible outside it, and may be preempted.
    Preemptible,
    /// Defined inside the module, and visible outside it.
    Export,
}

impl Linkage {
    fn merge(a: Self, b: Self) -> Self {
        match a {
            Linkage::Export => Linkage::Export,
            Linkage::Preemptible => match b {
                Linkage::Export => Linkage::Export,
                _ => Linkage::Preemptible,
            },
            Linkage::Local => match b {
                Linkage::Export => Linkage::Export,
                Linkage::Preemptible => Linkage::Preemptible,
                _ => Linkage::Local,
            },
            Linkage::Import => b,
        }
    }

    /// Test whether this linkage can have a definition.
    pub fn is_definable(self) -> bool {
        match self {
            Linkage::Import => false,
            Linkage::Local | Linkage::Preemptible | Linkage::Export => true,
        }
    }

    /// Test whether this linkage will have a definition that cannot be preempted.
    pub fn is_final(self) -> bool {
        match self {
            Linkage::Import | Linkage::Preemptible => false,
            Linkage::Local | Linkage::Export => true,
        }
    }
}

/// A declared name may refer to either a function or data declaration
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum FuncOrDataId {
    /// When it's a FuncId
    Func(FuncId),
    /// When it's a DataId
    Data(DataId),
}

/// Mapping to `ir::ExternalName` is trivial based on the `FuncId` and `DataId` mapping.
impl From<FuncOrDataId> for ir::ExternalName {
    fn from(id: FuncOrDataId) -> Self {
        match id {
            FuncOrDataId::Func(funcid) => Self::from(funcid),
            FuncOrDataId::Data(dataid) => Self::from(dataid),
        }
    }
}

/// Information about a function which can be called.
pub struct FunctionDeclaration {
    pub name: String,
    pub linkage: Linkage,
    pub signature: ir::Signature,
}

/// Error messages for all `Module` and `Backend` methods
#[derive(Fail, Debug)]
pub enum ModuleError {
    /// Indicates an identifier was used before it was declared
    #[fail(display = "Undeclared identifier: {}", _0)]
    Undeclared(String),
    /// Indicates an identifier was used contrary to the way it was declared
    #[fail(display = "Incompatible declaration of identifier: {}", _0)]
    IncompatibleDeclaration(String),
    /// Indicates an identifier was defined more than once
    #[fail(display = "Duplicate definition of identifier: {}", _0)]
    DuplicateDefinition(String),
    /// Indicates an identifier was defined, but was declared as an import
    #[fail(display = "Invalid to define identifier declared as an import: {}", _0)]
    InvalidImportDefinition(String),
    /// Wraps a `cranelift-codegen` error
    #[fail(display = "Compilation error: {}", _0)]
    Compilation(CodegenError),
    /// Wraps a generic error from a backend
    #[fail(display = "Backend error: {}", _0)]
    Backend(String),
}

/// A convenient alias for a `Result` that uses `ModuleError` as the error type.
pub type ModuleResult<T> = Result<T, ModuleError>;

/// A function belonging to a `Module`.
struct ModuleFunction<B>
where
    B: Backend,
{
    /// The function declaration.
    decl: FunctionDeclaration,
    /// The compiled artifact, once it's available.
    compiled: Option<B::CompiledFunction>,
    /// A flag indicating whether the function has been finalized.
    finalized: bool,
}

impl<B> ModuleFunction<B>
where
    B: Backend,
{
    fn merge(&mut self, linkage: Linkage, sig: &ir::Signature) -> Result<(), ModuleError> {
        self.decl.linkage = Linkage::merge(self.decl.linkage, linkage);
        if &self.decl.signature != sig {
            return Err(ModuleError::IncompatibleDeclaration(self.decl.name.clone()));
        }
        Ok(())
    }
}

/// Information about a data object which can be accessed.
pub struct DataDeclaration {
    pub name: String,
    pub linkage: Linkage,
    pub writable: bool,
}

/// A data object belonging to a `Module`.
struct ModuleData<B>
where
    B: Backend,
{
    /// The data object declaration.
    decl: DataDeclaration,
    /// The "compiled" artifact, once it's available.
    compiled: Option<B::CompiledData>,
    /// A flag indicating whether the data object has been finalized.
    finalized: bool,
}

impl<B> ModuleData<B>
where
    B: Backend,
{
    fn merge(&mut self, linkage: Linkage, writable: bool) {
        self.decl.linkage = Linkage::merge(self.decl.linkage, linkage);
        self.decl.writable = self.decl.writable || writable;
    }
}

/// The functions and data objects belonging to a module.
struct ModuleContents<B>
where
    B: Backend,
{
    functions: PrimaryMap<FuncId, ModuleFunction<B>>,
    data_objects: PrimaryMap<DataId, ModuleData<B>>,
}

impl<B> ModuleContents<B>
where
    B: Backend,
{
    fn get_function_info(&self, name: &ir::ExternalName) -> &ModuleFunction<B> {
        if let ir::ExternalName::User { namespace, index } = *name {
            debug_assert_eq!(namespace, 0);
            let func = FuncId::new(index as usize);
            &self.functions[func]
        } else {
            panic!("unexpected ExternalName kind {}", name)
        }
    }

    /// Get the `DataDeclaration` for the function named by `name`.
    fn get_data_info(&self, name: &ir::ExternalName) -> &ModuleData<B> {
        if let ir::ExternalName::User { namespace, index } = *name {
            debug_assert_eq!(namespace, 1);
            let data = DataId::new(index as usize);
            &self.data_objects[data]
        } else {
            panic!("unexpected ExternalName kind {}", name)
        }
    }
}

/// This provides a view to the state of a module which allows `ir::ExternalName`s to be translated
/// into `FunctionDeclaration`s and `DataDeclaration`s.
pub struct ModuleNamespace<'a, B: 'a>
where
    B: Backend,
{
    contents: &'a ModuleContents<B>,
}

impl<'a, B> ModuleNamespace<'a, B>
where
    B: Backend,
{
    /// Get the `FunctionDeclaration` for the function named by `name`.
    pub fn get_function_decl(&self, name: &ir::ExternalName) -> &FunctionDeclaration {
        &self.contents.get_function_info(name).decl
    }

    /// Get the `DataDeclaration` for the function named by `name`.
    pub fn get_data_decl(&self, name: &ir::ExternalName) -> &DataDeclaration {
        &self.contents.get_data_info(name).decl
    }

    /// Get the definition for the function named by `name`, along with its name
    /// and signature.
    pub fn get_function_definition(
        &self,
        name: &ir::ExternalName,
    ) -> (Option<&B::CompiledFunction>, &str, &ir::Signature) {
        let info = self.contents.get_function_info(name);
        debug_assert_eq!(info.decl.linkage.is_definable(), info.compiled.is_some());
        (
            info.compiled.as_ref(),
            &info.decl.name,
            &info.decl.signature,
        )
    }

    /// Get the definition for the data object named by `name`, along with its name
    /// and writable flag
    pub fn get_data_definition(
        &self,
        name: &ir::ExternalName,
    ) -> (Option<&B::CompiledData>, &str, bool) {
        let info = self.contents.get_data_info(name);
        debug_assert_eq!(info.decl.linkage.is_definable(), info.compiled.is_some());
        (info.compiled.as_ref(), &info.decl.name, info.decl.writable)
    }

    /// Return whether `name` names a function, rather than a data object.
    pub fn is_function(&self, name: &ir::ExternalName) -> bool {
        if let ir::ExternalName::User { namespace, .. } = *name {
            namespace == 0
        } else {
            panic!("unexpected ExternalName kind {}", name)
        }
    }
}

/// A `Module` is a utility for collecting functions and data objects, and linking them together.
pub struct Module<B>
where
    B: Backend,
{
    names: HashMap<String, FuncOrDataId>,
    contents: ModuleContents<B>,
    backend: B,
}

impl<B> Module<B>
where
    B: Backend,
{
    /// Create a new `Module`.
    pub fn new(backend_builder: B::Builder) -> Self {
        Self {
            names: HashMap::new(),
            contents: ModuleContents {
                functions: PrimaryMap::new(),
                data_objects: PrimaryMap::new(),
            },
            backend: B::new(backend_builder),
        }
    }

    /// Get the module identifier for a given name, if that name
    /// has been declared.
    pub fn get_name(&self, name: &str) -> Option<FuncOrDataId> {
        self.names.get(name).cloned()
    }

    /// Return then pointer type for the current target.
    pub fn pointer_type(&self) -> ir::types::Type {
        self.backend.isa().pointer_type()
    }

    /// Create a new `Context` initialized for use with this `Module`.
    ///
    /// This ensures that the `Context` is initialized with the default calling
    /// convention for the `TargetIsa`.
    pub fn make_context(&self) -> Context {
        let mut ctx = Context::new();
        ctx.func.signature.call_conv = self.backend.isa().flags().call_conv();
        ctx
    }

    /// Create a new `Context` initialized for use with this `Module`.
    ///
    /// This ensures that the `Context` is initialized with the default calling
    /// convention for the `TargetIsa`.
    pub fn clear_context(&self, ctx: &mut Context) {
        ctx.clear();
        ctx.func.signature.call_conv = self.backend.isa().flags().call_conv();
    }

    /// Declare a function in this module.
    pub fn declare_function(
        &mut self,
        name: &str,
        linkage: Linkage,
        signature: &ir::Signature,
    ) -> ModuleResult<FuncId> {
        // TODO: Can we avoid allocating names so often?
        use std::collections::hash_map::Entry::*;
        match self.names.entry(name.to_owned()) {
            Occupied(entry) => match *entry.get() {
                FuncOrDataId::Func(id) => {
                    let existing = &mut self.contents.functions[id];
                    existing.merge(linkage, signature)?;
                    self.backend.declare_function(name, existing.decl.linkage);
                    Ok(id)
                }
                FuncOrDataId::Data(..) => {
                    Err(ModuleError::IncompatibleDeclaration(name.to_owned()))
                }
            },
            Vacant(entry) => {
                let id = self.contents.functions.push(ModuleFunction {
                    decl: FunctionDeclaration {
                        name: name.to_owned(),
                        linkage,
                        signature: signature.clone(),
                    },
                    compiled: None,
                    finalized: false,
                });
                entry.insert(FuncOrDataId::Func(id));
                self.backend.declare_function(name, linkage);
                Ok(id)
            }
        }
    }

    /// Declare a data object in this module.
    pub fn declare_data(
        &mut self,
        name: &str,
        linkage: Linkage,
        writable: bool,
    ) -> ModuleResult<DataId> {
        // TODO: Can we avoid allocating names so often?
        use std::collections::hash_map::Entry::*;
        match self.names.entry(name.to_owned()) {
            Occupied(entry) => match *entry.get() {
                FuncOrDataId::Data(id) => {
                    let existing = &mut self.contents.data_objects[id];
                    existing.merge(linkage, writable);
                    self.backend
                        .declare_data(name, existing.decl.linkage, existing.decl.writable);
                    Ok(id)
                }

                FuncOrDataId::Func(..) => {
                    Err(ModuleError::IncompatibleDeclaration(name.to_owned()))
                }
            },
            Vacant(entry) => {
                let id = self.contents.data_objects.push(ModuleData {
                    decl: DataDeclaration {
                        name: name.to_owned(),
                        linkage,
                        writable,
                    },
                    compiled: None,
                    finalized: false,
                });
                entry.insert(FuncOrDataId::Data(id));
                self.backend.declare_data(name, linkage, writable);
                Ok(id)
            }
        }
    }

    /// Use this when you're building the IR of a function to reference a function.
    ///
    /// TODO: Coalesce redundant decls and signatures.
    /// TODO: Look into ways to reduce the risk of using a FuncRef in the wrong function.
    pub fn declare_func_in_func(&self, func: FuncId, in_func: &mut ir::Function) -> ir::FuncRef {
        let decl = &self.contents.functions[func].decl;
        let signature = in_func.import_signature(decl.signature.clone());
        let colocated = decl.linkage.is_final();
        in_func.import_function(ir::ExtFuncData {
            name: ir::ExternalName::user(0, func.index() as u32),
            signature,
            colocated,
        })
    }

    /// Use this when you're building the IR of a function to reference a data object.
    ///
    /// TODO: Same as above.
    pub fn declare_data_in_func(&self, data: DataId, func: &mut ir::Function) -> ir::GlobalValue {
        let decl = &self.contents.data_objects[data].decl;
        let colocated = decl.linkage.is_final();
        func.create_global_value(ir::GlobalValueData::Sym {
            name: ir::ExternalName::user(1, data.index() as u32),
            colocated,
        })
    }

    /// TODO: Same as above.
    pub fn declare_func_in_data(&self, func: FuncId, ctx: &mut DataContext) -> ir::FuncRef {
        ctx.import_function(ir::ExternalName::user(0, func.index() as u32))
    }

    /// TODO: Same as above.
    pub fn declare_data_in_data(&self, data: DataId, ctx: &mut DataContext) -> ir::GlobalValue {
        ctx.import_global_value(ir::ExternalName::user(1, data.index() as u32))
    }

    /// Define a function, producing the function body from the given `Context`.
    pub fn define_function(&mut self, func: FuncId, ctx: &mut Context) -> ModuleResult<()> {
        let compiled = {
            let code_size = ctx.compile(self.backend.isa()).map_err(|e| {
                info!(
                    "defining function {}: {}",
                    func,
                    ctx.func.display(self.backend.isa())
                );
                ModuleError::Compilation(e)
            })?;

            let info = &self.contents.functions[func];
            if info.compiled.is_some() {
                return Err(ModuleError::DuplicateDefinition(info.decl.name.clone()));
            }
            if !info.decl.linkage.is_definable() {
                return Err(ModuleError::InvalidImportDefinition(info.decl.name.clone()));
            }
            Some(self.backend.define_function(
                &info.decl.name,
                ctx,
                &ModuleNamespace::<B> {
                    contents: &self.contents,
                },
                code_size,
            )?)
        };
        self.contents.functions[func].compiled = compiled;
        Ok(())
    }

    /// Define a function, producing the data contents from the given `DataContext`.
    pub fn define_data(&mut self, data: DataId, data_ctx: &DataContext) -> ModuleResult<()> {
        let compiled = {
            let info = &self.contents.data_objects[data];
            if info.compiled.is_some() {
                return Err(ModuleError::DuplicateDefinition(info.decl.name.clone()));
            }
            if !info.decl.linkage.is_definable() {
                return Err(ModuleError::InvalidImportDefinition(info.decl.name.clone()));
            }
            Some(self.backend.define_data(
                &info.decl.name,
                info.decl.writable,
                data_ctx,
                &ModuleNamespace::<B> {
                    contents: &self.contents,
                },
            )?)
        };
        self.contents.data_objects[data].compiled = compiled;
        Ok(())
    }

    /// Write the address of `what` into the data for `data` at `offset`. `data` must refer to a
    /// defined data object.
    pub fn write_data_funcaddr(&mut self, data: DataId, offset: usize, what: ir::FuncRef) {
        let info = &mut self.contents.data_objects[data];
        debug_assert!(
            info.decl.linkage.is_definable(),
            "imported data cannot contain references"
        );
        self.backend.write_data_funcaddr(
            &mut info
                .compiled
                .as_mut()
                .expect("`data` must refer to a defined data object"),
            offset,
            what,
        );
    }

    /// Write the address of `what` plus `addend` into the data for `data` at `offset`. `data` must
    /// refer to a defined data object.
    pub fn write_data_dataaddr(
        &mut self,
        data: DataId,
        offset: usize,
        what: ir::GlobalValue,
        addend: binemit::Addend,
    ) {
        let info = &mut self.contents.data_objects[data];
        debug_assert!(
            info.decl.linkage.is_definable(),
            "imported data cannot contain references"
        );
        self.backend.write_data_dataaddr(
            &mut info
                .compiled
                .as_mut()
                .expect("`data` must refer to a defined data object"),
            offset,
            what,
            addend,
        );
    }

    /// Perform all outstanding relocations on the given function. This requires all `Local`
    /// and `Export` entities referenced to be defined.
    ///
    /// # Panics
    ///
    /// When the function has already been finalized this panics
    pub fn finalize_function(&mut self, func: FuncId) -> B::FinalizedFunction {
        let output = {
            let info = &self.contents.functions[func];
            debug_assert!(
                info.decl.linkage.is_definable(),
                "imported function cannot be finalized"
            );
            assert!(!info.finalized, "function can't be finalized twice");
            self.backend.finalize_function(
                info.compiled
                    .as_ref()
                    .expect("function must be compiled before it can be finalized"),
                &ModuleNamespace::<B> {
                    contents: &self.contents,
                },
            )
        };
        self.contents.functions[func].finalized = true;
        output
    }

    /// Perform all outstanding relocations on the given data object. This requires all
    /// `Local` and `Export` entities referenced to be defined.
    ///
    /// # Panics
    ///
    /// When the data object has already been finalized this panics
    pub fn finalize_data(&mut self, data: DataId) -> B::FinalizedData {
        let output = {
            let info = &self.contents.data_objects[data];
            debug_assert!(
                info.decl.linkage.is_definable(),
                "imported data cannot be finalized"
            );
            assert!(!info.finalized, "data object can't be finalized twice");
            self.backend.finalize_data(
                info.compiled
                    .as_ref()
                    .expect("data object must be compiled before it can be finalized"),
                &ModuleNamespace::<B> {
                    contents: &self.contents,
                },
            )
        };
        self.contents.data_objects[data].finalized = true;
        output
    }

    /// Finalize all functions and data objects. Note that this doesn't return the
    /// final artifacts returned from `finalize_function` or `finalize_data`.
    pub fn finalize_all(&mut self) {
        // TODO: Could we use something like `into_iter()` here?
        for info in self.contents.functions.values() {
            if info.decl.linkage.is_definable() && !info.finalized {
                self.backend.finalize_function(
                    info.compiled
                        .as_ref()
                        .expect("function must be compiled before it can be finalized"),
                    &ModuleNamespace::<B> {
                        contents: &self.contents,
                    },
                );
            }
        }
        for info in self.contents.data_objects.values() {
            if info.decl.linkage.is_definable() && !info.finalized {
                self.backend.finalize_data(
                    info.compiled
                        .as_ref()
                        .expect("data object must be compiled before it can be finalized"),
                    &ModuleNamespace::<B> {
                        contents: &self.contents,
                    },
                );
            }
        }
    }

    /// Consume the module and return the resulting `Product`. Some `Backend`
    /// implementations may provide additional functionality available after
    /// a `Module` is complete.
    pub fn finish(self) -> B::Product {
        self.backend.finish()
    }
}
