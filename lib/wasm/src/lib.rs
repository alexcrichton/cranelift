//! Performs the translation from a wasm module in binary format to the in-memory representation
//! of the Cretonne IL. More particularly, it translates the code of all the functions bodies and
//! interacts with a runtime implementing the [`ModuleEnvironment`](trait.ModuleEnvironment.html)
//! trait to deal with tables, globals and linear memory.
//!
//! The crate provides a `DummyEnvironment` struct that will allow to translate the code of the
//! functions but will fail at execution.
//!
//! The main function of this module is [`translate_module`](fn.translate_module.html).

#![deny(missing_docs)]

extern crate wasmparser;
extern crate cton_frontend;
#[macro_use(dbg)]
extern crate cretonne;

mod code_translator;
mod func_translator;
mod module_translator;
mod runtime;
mod sections_translator;
mod state;
mod translation_utils;

pub use func_translator::FuncTranslator;
pub use module_translator::translate_module;
pub use runtime::{FuncEnvironment, ModuleEnvironment, DummyEnvironment, GlobalValue};
pub use translation_utils::{FunctionIndex, GlobalIndex, TableIndex, MemoryIndex, SignatureIndex,
                            Global, GlobalInit, Table, Memory};
