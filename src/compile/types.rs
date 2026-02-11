use std::{
    fmt::Display,
    ops::RangeInclusive,
    collections::HashMap,
};

use crate::intermediate::{EnumMember, SourceLocation, TypeName};

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub ty: TypeID,
    pub defined_at: SourceLocation,
}

/// Plain-old-data aggregate with C-like layout.
#[derive(Debug)]
pub struct Pack {
    pub members: Vec<StructMember>,
}

/// Enum with an explicit primitive underlying type.
#[derive(Debug)]
pub struct Enum {
    pub underlying: TypeID,
    pub members: Vec<EnumMember>,
    pub default: Option<EnumMember>,
}

#[derive(Debug)]
pub struct BitfldMember {
    pub name: String,
    pub underlying: TypeID,
    pub range: RangeInclusive<u32>,
    pub defined_at: SourceLocation,
}

/// Bitfield backed by an integer/enum type with named bit ranges.
#[derive(Debug)]
pub struct Bitfld {
    pub underlying: TypeID,
    pub members: Vec<BitfldMember>,
}

#[derive(Debug)]
pub struct VariantMember {
    pub ty: TypeID,
    pub value: u64,
    pub defined_at: SourceLocation,
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub discriminant: TypeID,
    pub members: Vec<VariantMember>,
}

/// Sequence of named fields (a typical record/struct).
#[derive(Debug)]
pub struct Sequence {
    pub members: Vec<StructMember>,
}

#[derive(Debug)]
pub struct Alias {
    pub other: TypeID,
}

/// Array kinds
#[derive(Debug)]
pub struct DynamicArray {
    pub size_type: TypeID,
    pub value_type: TypeID,
}

impl DynamicArray {
    /// Returns whether this array's element type is POD (used to decide aggregation behavior).
    pub fn is_aggregate(&self, world: &World) -> bool {
        world.lookup(self.value_type).is_pod(world)
    }
}

#[derive(Debug)]
pub struct FixedArray {
    pub count: u64,
    pub value_type: TypeID,
}

impl FixedArray {
    /// Returns whether this array's element type is POD (used to decide aggregation behavior).
    pub fn is_aggregate(&self, world: &World) -> bool {
        world.lookup(self.value_type).is_pod(world)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BitWidth {
    W8,
    W16,
    W32,
    W64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signedness {
    Unsigned,
    Signed,
}

#[derive(Debug, Clone, Copy)]
pub enum Datatype {
    Integer,
    Float,
}

#[derive(Debug, Clone, Copy)]
pub struct Primitive {
    pub width: BitWidth,
    pub sign: Signedness,
    pub dtype: Datatype,
}

impl Primitive {
    /// Returns the bit width of this primitive as a concrete integer.
    pub(crate) fn bits(&self) -> u32 {
        match self.width {
            BitWidth::W8 => 8,
            BitWidth::W16 => 16,
            BitWidth::W32 => 32,
            BitWidth::W64 => 64,
        }
    }

    /// Returns whether this primitive is exactly `u8`.
    pub fn is_u8(&self) -> bool {
        matches!(
            (self.width, self.sign, self.dtype),
            (BitWidth::W8, Signedness::Unsigned, Datatype::Integer)
        )
    }

    /// Validates that an integer value fits into this primitive's representable range.
    pub(crate) fn verify_can_fit(&self, v: i128) -> anyhow::Result<()> {
        let Datatype::Integer = self.dtype else {
            anyhow::bail!("integer cannot fit in a float")
        };

        let bounds = match (self.width, self.sign) {
            (BitWidth::W8, Signedness::Unsigned) => (0i128, u8::MAX as i128),
            (BitWidth::W8, Signedness::Signed) => (i8::MIN as i128, i8::MAX as i128),
            (BitWidth::W16, Signedness::Unsigned) => (0i128, u16::MAX as i128),
            (BitWidth::W16, Signedness::Signed) => (i16::MIN as i128, i16::MAX as i128),
            (BitWidth::W32, Signedness::Unsigned) => (0i128, u32::MAX as i128),
            (BitWidth::W32, Signedness::Signed) => (i32::MIN as i128, i32::MAX as i128),
            (BitWidth::W64, Signedness::Unsigned) => (0i128, u64::MAX as i128),
            (BitWidth::W64, Signedness::Signed) => (i64::MIN as i128, i64::MAX as i128),
        };

        if v >= bounds.0 && v <= bounds.1 {
            return Ok(());
        }

        anyhow::bail!("value {v} cannot fit in a primitive of {}", self)
    }
}

impl Display for Primitive {
    /// Formats a primitive using the DSL's scalar naming convention (e.g. `u32`, `f64`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ts = match (self.dtype, self.sign) {
            (Datatype::Integer, Signedness::Unsigned) => "u",
            (Datatype::Integer, Signedness::Signed) => "i",
            (Datatype::Float, Signedness::Unsigned) => "f",
            (Datatype::Float, Signedness::Signed) => "f",
        };
        let w = match self.width {
            BitWidth::W8 => "8",
            BitWidth::W16 => "16",
            BitWidth::W32 => "32",
            BitWidth::W64 => "64",
        };
        write!(f, "{ts}{w}")
    }
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
    Primitive(Primitive),
    Void,
}

#[derive(Debug)]
pub struct Type {
    pub ident: TypeName,
    pub defined_at: SourceLocation,
    pub kind: TypeKind,
}

impl Type {
    /// Returns whether this type is POD (plain-old-data) under the DSL rules.
    fn is_pod(&self, world: &World) -> bool {
        match &self.kind {
            TypeKind::Alias(alias) => world.lookup(alias.other).is_pod(world),
            TypeKind::Pack(_) => true,
            TypeKind::Enum(_) => true,
            TypeKind::Bitfld(_) => true,
            TypeKind::Variant(_) => false,
            TypeKind::Sequence(_) => false,
            TypeKind::DynamicArray(_) => false,
            TypeKind::FixedArray(x) => world.lookup(x.value_type).is_pod(world),
            TypeKind::Primitive(_) => true,
            TypeKind::Void => false,
        }
    }
}

// Ordered so we can use TypeIDs directly as stable sort keys.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct TypeID(u32);

#[derive(Debug)]
pub struct World {
    pub(crate) definitions: HashMap<TypeID, Type>,
    pub(crate) sorted: Vec<TypeID>,
    pub(crate) module_name: String,
}

impl World {
    /// Looks up a type by `TypeID`.
    pub fn lookup(&self, tname: TypeID) -> &Type {
        self.definitions.get(&tname).unwrap()
    }

    /// Iterates compiled type definitions in dependency order.
    pub fn iter(&self) -> impl Iterator<Item = (TypeID, &Type)> {
        self.sorted
            .iter()
            .filter_map(|id| self.definitions.get(id).map(|ty| (*id, ty)))
    }

    /// Returns the module name from the parsed input.
    pub fn module_name(&self) -> &str {
        &self.module_name
    }
}
