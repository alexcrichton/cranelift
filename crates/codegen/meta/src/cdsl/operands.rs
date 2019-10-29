use std::collections::HashMap;

use crate::cdsl::camel_case;
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
    pub name: &'static str,
    doc: Option<&'static str>,
    pub kind: OperandKind,
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
    pub name: &'static str,

    doc: Option<String>,

    pub default_member: Option<&'static str>,

    /// The camel-cased name of an operand kind is also the Rust type used to represent it.
    pub rust_type: String,

    pub fields: OperandKindFields,
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

    pub fn imm_name(&self) -> Option<&str> {
        match self.fields {
            OperandKindFields::ImmEnum(_)
            | OperandKindFields::ImmValue
            | OperandKindFields::EntityRef => Some(&self.name),
            _ => None,
        }
    }
}

pub(crate) struct OperandKindBuilder {
    name: &'static str,

    doc: Option<String>,

    default_member: Option<&'static str>,

    /// The camel-cased name of an operand kind is also the Rust type used to represent it.
    rust_type: Option<String>,

    fields: OperandKindFields,
}

impl OperandKindBuilder {
    pub fn new(name: &'static str, fields: OperandKindFields) -> Self {
        Self {
            name,
            doc: None,
            default_member: None,
            rust_type: None,
            fields,
        }
    }

    pub fn new_imm(name: &'static str) -> Self {
        Self {
            name,
            doc: None,
            default_member: None,
            rust_type: None,
            fields: OperandKindFields::ImmValue,
        }
    }

    pub fn new_enum(name: &'static str, values: EnumValues) -> Self {
        Self {
            name,
            doc: None,
            default_member: None,
            rust_type: None,
            fields: OperandKindFields::ImmEnum(values),
        }
    }

    pub fn doc(mut self, doc: &'static str) -> Self {
        assert!(self.doc.is_none());
        self.doc = Some(doc.to_string());
        self
    }
    pub fn default_member(mut self, default_member: &'static str) -> Self {
        assert!(self.default_member.is_none());
        self.default_member = Some(default_member);
        self
    }
    pub fn rust_type(mut self, rust_type: &'static str) -> Self {
        assert!(self.rust_type.is_none());
        self.rust_type = Some(rust_type.to_string());
        self
    }

    pub fn build(self) -> OperandKind {
        let default_member = match self.default_member {
            Some(default_member) => Some(default_member),
            None => match &self.fields {
                OperandKindFields::ImmEnum(_) | OperandKindFields::ImmValue => Some("imm"),
                OperandKindFields::TypeVar(_) | OperandKindFields::EntityRef => Some(self.name),
                OperandKindFields::VariableArgs => None,
            },
        };

        let rust_type = match self.rust_type {
            Some(rust_type) => rust_type.to_string(),
            None => match &self.fields {
                OperandKindFields::ImmEnum(_) | OperandKindFields::ImmValue => {
                    format!("ir::immediates::{}", camel_case(self.name))
                }
                OperandKindFields::VariableArgs => "&[Value]".to_string(),
                OperandKindFields::TypeVar(_) | OperandKindFields::EntityRef => {
                    format!("ir::{}", camel_case(self.name))
                }
            },
        };

        OperandKind {
            name: self.name,
            doc: self.doc,
            default_member,
            rust_type,
            fields: self.fields,
        }
    }
}

impl Into<OperandKind> for &TypeVar {
    fn into(self) -> OperandKind {
        OperandKindBuilder::new("value", OperandKindFields::TypeVar(self.into())).build()
    }
}
impl Into<OperandKind> for &OperandKind {
    fn into(self) -> OperandKind {
        self.clone()
    }
}
