use std::fmt::Display;

use super::source::SourceLocation;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct TypeName(std::sync::Arc<String>);

impl TypeName {
    /// Returns the underlying string representation of the type name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Creates a `TypeName` from a string slice (trimming whitespace).
    pub fn from_string(
        line: SourceLocation,
        x: impl AsRef<str>,
    ) -> Result<Self, super::IntermediateError> {
        let slice: &str = x.as_ref();
        if let Some(x) = slice.chars().find(|x| !is_legal_typename_char(*x)) {
            return Err(super::IntermediateError::IllegalChar { line, reason: x });
        }

        Ok(Self(std::sync::Arc::new(slice.trim().into())))
    }
}

impl Display for TypeName {
    /// Formats a type name as it appears in the DSL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn is_legal_typename_char(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')
}

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub ty: TypeName,
    pub defined_at: SourceLocation,
}

/// Plain-old-data aggregate with C-like layout.
#[derive(Debug)]
pub struct Pack {
    pub members: Vec<StructMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumMember {
    pub name: String,
    pub value: i64,
    pub defined_at: SourceLocation,
}

/// Enum with an explicit primitive underlying type.
#[derive(Debug)]
pub struct Enum {
    pub ty: TypeName,
    pub members: Vec<EnumMember>,
    pub default: Option<EnumMember>,
}

#[derive(Debug)]
pub struct BitfldMember {
    pub name: String,
    pub ty: TypeName,
    pub range: String,
    pub defined_at: SourceLocation,
}

/// Bitfield backed by an integer/enum type with named bit ranges.
#[derive(Debug)]
pub struct Bitfld {
    pub ty: TypeName,
    pub members: Vec<BitfldMember>,
}

#[derive(Debug)]
pub struct VariantMember {
    pub ty: TypeName,
    pub value: u64,
    pub defined_at: SourceLocation,
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub ty: TypeName,
    pub members: Vec<VariantMember>,
}

/// Sequence of named fields (a typical record/struct).
#[derive(Debug)]
pub struct Sequence {
    pub members: Vec<StructMember>,
}

#[derive(Debug)]
pub struct Alias {
    pub other: TypeName,
}

/// Array kinds
#[derive(Debug)]
pub struct DynamicArray {
    pub size_type: TypeName,
    pub value_type: TypeName,
}

#[derive(Debug)]
pub struct FixedArray {
    pub count: u64,
    pub value_type: TypeName,
}

#[derive(Debug)]
pub enum TypeKind {
    Alias(Alias),
    Pack(Pack),
    Enum(Enum),
    Bitfld(Bitfld),
    Variant(Variant),
    Sequence(Sequence),
    DynamicArray(DynamicArray),
    FixedArray(FixedArray),
}

#[derive(Debug)]
pub struct Type {
    pub ident: TypeName,
    pub defined_at: SourceLocation,
    pub kind: TypeKind,
}

#[derive(Debug)]
pub struct Module {
    pub name: String,

    pub source: String,

    pub imports: Vec<Import>,

    pub definitions: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub types: Vec<TypeName>,
    pub defined_at: SourceLocation,
}
