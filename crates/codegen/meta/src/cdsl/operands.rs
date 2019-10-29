use std::collections::HashMap;

use crate::cdsl::typevar::TypeVar;

/// An instruction operand can be an *immediate*, an *SSA value*, or an *entity reference*. The
/// type of the operand is one of:
///
/// 1. A `ValueType` instance indicates an SSA value operand with a concrete type.
///
/// 2. A `TypeVar` instance indicates an SSA value operand, and the instruction is polymorphic over
///    the possible concrete types that the type variable can assume.
///
/// 3. An `ImmediateKind` instance indicates an immediate operand whose value is encoded in the
///    instruction itself rather than being passed as an SSA value.
///
/// 4. An `EntityRefKind` instance indicates an operand that references another entity in the
///    function, typically something declared in the function preamble.
#[derive(Clone, Debug)]
pub(crate) struct Operand {
    /// Name of the operand variable, as it appears in function parameters, legalizations, etc.
    pub name: &'static str,

    /// Type of the operand.
    pub kind: OperandKind,

    doc: Option<&'static str>,
}

impl Operand {
    pub fn new(name: &'static str, kind: impl Into<OperandKind>) -> Self {
        Self {
            name,
            doc: None,
            kind: kind.into(),
        }
    }
    pub fn with_doc(mut self, doc: &'static str) -> Self {
        self.doc = Some(doc);
        self
    }

    pub fn doc(&self) -> Option<&str> {
        if let Some(doc) = &self.doc {
            return Some(doc);
        }
        match &self.kind.fields {
            OperandKindFields::TypeVar(tvar) => Some(&tvar.doc),
            _ => self.kind.doc(),
        }
    }

    pub fn is_value(&self) -> bool {
        match self.kind.fields {
            OperandKindFields::TypeVar(_) => true,
            _ => false,
        }
    }

    pub fn type_var(&self) -> Option<&TypeVar> {
        match &self.kind.fields {
            OperandKindFields::TypeVar(typevar) => Some(typevar),
            _ => None,
        }
    }

    pub fn is_varargs(&self) -> bool {
        match self.kind.fields {
            OperandKindFields::VariableArgs => true,
            _ => false,
        }
    }

    /// Returns true if the operand has an immediate kind or is an EntityRef.
    pub fn is_immediate_or_entityref(&self) -> bool {
        match self.kind.fields {
            OperandKindFields::ImmEnum(_)
            | OperandKindFields::ImmValue
            | OperandKindFields::EntityRef => true,
            _ => false,
        }
    }

    /// Returns true if the operand has an immediate kind.
    pub fn is_immediate(&self) -> bool {
        match self.kind.fields {
            OperandKindFields::ImmEnum(_) | OperandKindFields::ImmValue => true,
            _ => false,
        }
    }

    pub fn is_cpu_flags(&self) -> bool {
        match &self.kind.fields {
            OperandKindFields::TypeVar(type_var)
                if type_var.name == "iflags" || type_var.name == "fflags" =>
            {
                true
            }
            _ => false,
        }
    }
}

type EnumValues = HashMap<&'static str, &'static str>;

#[derive(Clone, Debug)]
pub(crate) enum OperandKindFields {
    EntityRef,
    VariableArgs,
    ImmValue,
    ImmEnum(EnumValues),
    TypeVar(TypeVar),
}

#[derive(Clone, Debug)]
pub(crate) struct OperandKind {
    /// String representation of the Rust type mapping to this OperandKind.
    pub rust_type: &'static str,

    /// Name of this OperandKind in the format's member field.
    pub rust_field_name: &'static str,

    /// Type-specific fields for this OperandKind.
    pub fields: OperandKindFields,

    doc: Option<&'static str>,
}

impl OperandKind {
    fn doc(&self) -> Option<&str> {
        if let Some(doc) = &self.doc {
            return Some(doc);
        }
        match &self.fields {
            OperandKindFields::TypeVar(type_var) => Some(&type_var.doc),
            OperandKindFields::ImmEnum(_)
            | OperandKindFields::ImmValue
            | OperandKindFields::EntityRef
            | OperandKindFields::VariableArgs => None,
        }
    }
}

impl Into<OperandKind> for &TypeVar {
    fn into(self) -> OperandKind {
        OperandKindBuilder::new(
            "value",
            "ir::Value",
            OperandKindFields::TypeVar(self.into()),
        )
        .build()
    }
}
impl Into<OperandKind> for &OperandKind {
    fn into(self) -> OperandKind {
        self.clone()
    }
}

pub(crate) struct OperandKindBuilder {
    rust_field_name: &'static str,
    rust_type: &'static str,
    fields: OperandKindFields,
    doc: Option<&'static str>,
}

impl OperandKindBuilder {
    pub fn new(
        rust_field_name: &'static str,
        rust_type: &'static str,
        fields: OperandKindFields,
    ) -> Self {
        Self {
            rust_field_name,
            rust_type,
            fields,
            doc: None,
        }
    }
    pub fn new_imm(rust_field_name: &'static str, rust_type: &'static str) -> Self {
        Self {
            rust_field_name,
            rust_type,
            fields: OperandKindFields::ImmValue,
            doc: None,
        }
    }
    pub fn new_enum(
        rust_field_name: &'static str,
        rust_type: &'static str,
        values: EnumValues,
    ) -> Self {
        Self {
            rust_field_name,
            rust_type,
            fields: OperandKindFields::ImmEnum(values),
            doc: None,
        }
    }
    pub fn with_doc(mut self, doc: &'static str) -> Self {
        assert!(self.doc.is_none());
        self.doc = Some(doc);
        self
    }
    pub fn build(self) -> OperandKind {
        OperandKind {
            rust_type: self.rust_type,
            fields: self.fields,
            rust_field_name: self.rust_field_name,
            doc: self.doc,
        }
    }
}
