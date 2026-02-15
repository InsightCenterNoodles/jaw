use std::{collections::HashMap, fmt::Display, ops::RangeInclusive};

use anyhow::bail;
use itertools::Itertools;

use crate::intermediate::{EnumMember, SourceLocation, TypeName};

#[derive(Debug, Clone, PartialEq)]
pub struct StructMember {
    pub name: String,
    pub ty: TypeID,
    pub defined_at: SourceLocation,
}

/// Plain-old-data aggregate with C-like layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub members: Vec<StructMember>,
}

/// Enum with an explicit primitive underlying type.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub underlying: TypeID,
    pub members: Vec<EnumMember>,
    pub default: Option<EnumMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitfldMember {
    pub name: String,
    pub underlying: TypeID,
    pub range: RangeInclusive<u32>,
    pub defined_at: SourceLocation,
}

/// Bitfield backed by an integer/enum type with named bit ranges.
#[derive(Debug, Clone, PartialEq)]
pub struct Bitfld {
    pub underlying: TypeID,
    pub members: Vec<BitfldMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantMember {
    pub ty: TypeID,
    pub value: u64,
    pub defined_at: SourceLocation,
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub discriminant: TypeID,
    pub members: Vec<VariantMember>,
}

/// Sequence of named fields (a typical record/struct).
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub members: Vec<StructMember>,
}

/// Array kinds
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Datatype {
    Integer,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
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

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub ident: TypeName,
    pub defined_at: SourceLocation,
    pub kind: TypeKind,
}

impl Type {
    /// Returns whether this type is POD (plain-old-data) under the DSL rules.
    fn is_pod(&self, world: &World) -> bool {
        match &self.kind {
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
pub struct TypeID(pub u32);

/// A compiled jaw module
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

    pub fn merge(self, mut other: World) -> anyhow::Result<World> {
        // check if we can actually merge
        for dup in self
            .definitions
            .values()
            .chain(other.definitions.values())
            .duplicates_by(|t| t.ident.clone())
            .cloned()
        {
            if matches!(dup.kind, TypeKind::Void | TypeKind::Primitive(_)) {
                continue;
            }

            // Find first def in self. Dups are detected in other

            let orig = self
                .definitions
                .values()
                .find(|x| x.ident == dup.ident)
                .expect("duplicated detected, but no duplicate found?");

            bail!(
                "type {} has conflicting definitions, first found at: {}, second found at: {}",
                dup.ident,
                orig.defined_at,
                dup.defined_at
            );
        }

        // ok, now execute merge

        // first we need a new type id offset

        let Some(max_tid) = self.sorted.iter().max().cloned() else {
            bail!("unable to remap type id while merging modules");
        };

        let new_base = max_tid.0;

        let remap = |id: &mut TypeID| id.0 += new_base;

        for x in &mut other.sorted {
            remap(x);
        }

        other.definitions = other
            .definitions
            .into_iter()
            .map(|(mut k, mut v)| {
                match &mut v.kind {
                    TypeKind::Pack(pack) => {
                        for m in &mut pack.members {
                            remap(&mut m.ty);
                        }
                    }
                    TypeKind::Enum(x) => remap(&mut x.underlying),
                    TypeKind::Bitfld(bitfld) => {
                        remap(&mut bitfld.underlying);

                        for x in &mut bitfld.members {
                            remap(&mut x.underlying);
                        }
                    }
                    TypeKind::Variant(variant) => {
                        remap(&mut variant.discriminant);

                        for x in &mut variant.members {
                            remap(&mut x.ty);
                        }
                    }
                    TypeKind::Sequence(sequence) => {
                        for x in &mut sequence.members {
                            remap(&mut x.ty);
                        }
                    }
                    TypeKind::DynamicArray(dynamic_array) => {
                        remap(&mut dynamic_array.size_type);
                        remap(&mut dynamic_array.value_type);
                    }
                    TypeKind::FixedArray(fixed_array) => {
                        remap(&mut fixed_array.value_type);
                    }
                    TypeKind::Primitive(_) => {}
                    TypeKind::Void => {}
                };

                remap(&mut k);

                (k, v)
            })
            .collect();

        Ok(World {
            definitions: self
                .definitions
                .into_iter()
                .chain(other.definitions.into_iter())
                .collect(),
            sorted: self
                .sorted
                .into_iter()
                .chain(other.sorted.into_iter())
                .collect(),
            module_name: self.module_name,
        })
    }
}
