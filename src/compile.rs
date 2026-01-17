use anyhow::{Context, anyhow, bail};
use itertools::Itertools;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    fmt::Display,
    ops::RangeInclusive,
};
use thiserror::Error;

use crate::intermediate::{
    self, EnumMember, Module, Position, SourceCode, SourceLocation, TypeName,
};

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
    fn bits(&self) -> u32 {
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
    fn verify_can_fit(&self, v: i128) -> anyhow::Result<()> {
        let Datatype::Integer = self.dtype else {
            bail!("integer cannot fit in a float")
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

        bail!("value {v} cannot fit in a primitive of {}", self)
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
    definitions: HashMap<TypeID, Type>,
    sorted: Vec<TypeID>,
    module_name: String,
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

struct TypeIDAllocator {
    last: u32,
}

impl TypeIDAllocator {
    /// Creates a new allocator.
    fn new() -> Self {
        Self { last: 0 }
    }

    /// Allocates the next available `TypeID`.
    fn next(&mut self) -> TypeID {
        let r = TypeID(self.last);
        self.last += 1;
        r
    }
}

struct CompileState {
    name_to_id: HashMap<TypeName, TypeID>,
}

impl CompileState {
    /// Resolves an intermediate `TypeName` to a compiled `TypeID`.
    fn lookup(&self, tname: &intermediate::TypeName) -> anyhow::Result<TypeID> {
        let Some(x) = self.name_to_id.get(tname) else {
            bail!("unknown type name `{tname}`");
        };
        Ok(*x)
    }
}

/// Parses a bitfield range like `"0"` or `"1-3"` into an inclusive numeric range.
fn string_to_range(range: String) -> anyhow::Result<RangeInclusive<u32>> {
    if let Some((a, b)) = range.split_once('-') {
        Ok(RangeInclusive::new(a.parse()?, b.parse()?))
    } else {
        let v = range.parse()?;
        Ok(RangeInclusive::new(v, v))
    }
}

/// Converts an intermediate type definition into the internal compiled representation.
fn convert(state: &CompileState, ty: intermediate::Type) -> anyhow::Result<(TypeID, Type)> {
    let this_id = state.lookup(&ty.ident)?;

    let new_kind = match ty.kind {
        intermediate::TypeKind::Alias(alias) => TypeKind::Alias(Alias {
            other: state
                .lookup(&alias.other)
                .with_context(|| format!("while resolving alias {}", ty.ident))?,
        }),
        intermediate::TypeKind::Pack(pack) => TypeKind::Pack(Pack {
            members: pack
                .members
                .into_iter()
                .map(|x| -> anyhow::Result<StructMember> {
                    let name_clone = x.name.clone();
                    Ok(StructMember {
                        name: x.name,
                        ty: state.lookup(&x.ty).with_context(|| {
                            format!(
                                "while resolving member {}.{} at {}",
                                ty.ident, name_clone, x.defined_at
                            )
                        })?,
                        defined_at: x.defined_at,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        }),
        intermediate::TypeKind::Enum(enm) => TypeKind::Enum(Enum {
            underlying: state
                .lookup(&enm.ty)
                .with_context(|| format!("while resolving enum base type for {}", ty.ident))?,
            members: enm.members,
            default: enm.default,
        }),
        intermediate::TypeKind::Bitfld(bitfld) => TypeKind::Bitfld(Bitfld {
            underlying: state
                .lookup(&bitfld.ty)
                .with_context(|| format!("while resolving bitfield base for {}", ty.ident))?,
            members: bitfld
                .members
                .into_iter()
                .map(|x| -> anyhow::Result<BitfldMember> {
                    let name_clone = x.name.clone();
                    Ok(BitfldMember {
                        name: x.name,
                        underlying: state.lookup(&x.ty).with_context(|| {
                            format!(
                                "while resolving bitfield member {}.{} at {}",
                                ty.ident, name_clone, x.defined_at
                            )
                        })?,
                        range: string_to_range(x.range).with_context(|| {
                            format!(
                                "while parsing bit range for {}.{} at {}",
                                ty.ident, name_clone, x.defined_at
                            )
                        })?,
                        defined_at: x.defined_at,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        }),
        intermediate::TypeKind::Variant(variant) => {
            let convert_member =
                |member: intermediate::VariantMember| -> anyhow::Result<VariantMember> {
                    Ok(VariantMember {
                        ty: state.lookup(&member.ty).with_context(|| {
                            format!(
                                "while resolving variant member type {} at {}",
                                member.ty, member.defined_at
                            )
                        })?,
                        value: member.value,
                        defined_at: member.defined_at,
                    })
                };

            TypeKind::Variant(Variant {
                discriminant: state.lookup(&variant.ty).with_context(|| {
                    format!("while resolving variant discriminant for {}", ty.ident)
                })?,
                members: variant
                    .members
                    .into_iter()
                    .map(convert_member)
                    .collect::<anyhow::Result<Vec<_>>>()?,
            })
        }
        intermediate::TypeKind::Sequence(sequence) => TypeKind::Sequence(Sequence {
            members: sequence
                .members
                .into_iter()
                .map(|x| {
                    let name_clone = x.name.clone();
                    Ok(StructMember {
                        name: x.name,
                        ty: state.lookup(&x.ty).with_context(|| {
                            format!(
                                "while resolving member {}.{} at {}",
                                ty.ident, name_clone, x.defined_at
                            )
                        })?,
                        defined_at: x.defined_at,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        }),
        intermediate::TypeKind::DynamicArray(dynamic_array) => {
            TypeKind::DynamicArray(DynamicArray {
                size_type: state.lookup(&dynamic_array.size_type).with_context(|| {
                    format!(
                        "while resolving dynamic array count type for {} at {}",
                        ty.ident, ty.defined_at
                    )
                })?,
                value_type: state.lookup(&dynamic_array.value_type).with_context(|| {
                    format!(
                        "while resolving dynamic array element type for {} at {}",
                        ty.ident, ty.defined_at
                    )
                })?,
            })
        }
        intermediate::TypeKind::FixedArray(fixed_array) => TypeKind::FixedArray(FixedArray {
            count: fixed_array.count,
            value_type: state.lookup(&fixed_array.value_type).with_context(|| {
                format!(
                    "while resolving fixed array element type for {} at {}",
                    ty.ident, ty.defined_at
                )
            })?,
        }),
    };

    Ok((
        this_id,
        Type {
            ident: ty.ident,
            defined_at: ty.defined_at,
            kind: new_kind,
        },
    ))
}

// MARK: Verify

#[derive(Debug)]
struct Typeref {
    name: TypeName,
    at: SourceLocation,
}

impl From<&Type> for Typeref {
    /// Constructs a diagnostic type reference from a compiled type.
    fn from(value: &Type) -> Self {
        Self {
            name: value.ident.clone(),
            at: value.defined_at.clone(),
        }
    }
}

impl Display for Typeref {
    /// Formats a type reference for error messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Symbol {} {}", self.name, self.at)
    }
}

#[derive(Debug, Error)]
enum NotPODError {
    #[error("Type {0} is not POD")]
    TypeNotPOD(Typeref),
}

/// Verifies that a type is POD, following aliases and container definitions.
fn verify_is_pod(world: &World, tid: TypeID) -> anyhow::Result<()> {
    let ty = world.lookup(tid);

    let ctx = || format!("while checking {}", Typeref::from(ty));

    match &ty.kind {
        TypeKind::Alias(alias) => verify_is_pod(world, alias.other).with_context(ctx)?,
        TypeKind::Pack(pack) => {
            for p in &pack.members {
                verify_is_pod(world, p.ty).with_context(ctx)?;
            }
        }
        TypeKind::Enum(x) => verify_is_pod(world, x.underlying).with_context(ctx)?,
        TypeKind::Bitfld(bitfld) => verify_is_pod(world, bitfld.underlying).with_context(ctx)?,
        TypeKind::FixedArray(fixed_array) => {
            verify_is_pod(world, fixed_array.value_type).with_context(ctx)?;
        }
        TypeKind::Primitive(..) => return Ok(()),
        _ => {
            return Err(NotPODError::TypeNotPOD(ty.into()).into());
        }
    };

    Ok(())
}

/// Asserts that a type is an integer with a specific signedness.
fn verify_is_int_of(world: &World, tid: TypeID, sign: Signedness) -> anyhow::Result<Primitive> {
    let ty = world.lookup(tid);

    let ctx = || format!("while checking {}", Typeref::from(ty));

    match &ty.kind {
        TypeKind::Alias(alias) => verify_is_int_of(world, alias.other, sign).with_context(ctx),
        TypeKind::Primitive(x) if matches!(x.dtype, Datatype::Integer) && x.sign == sign => Ok(*x),
        _ => Err(anyhow!(
            "type {} is not integer with sign {sign:?}",
            Typeref::from(ty)
        )
        .context(ctx())),
    }
}

/// Asserts that a type is an integer primitive (signed or unsigned).
fn verify_is_integer(world: &World, tid: TypeID) -> anyhow::Result<Primitive> {
    let ty = world.lookup(tid);

    let ctx = || format!("while checking {}", Typeref::from(ty));

    match &ty.kind {
        TypeKind::Alias(alias) => verify_is_integer(world, alias.other).with_context(ctx),
        TypeKind::Primitive(x) if matches!(x.dtype, Datatype::Integer) => Ok(*x),
        _ => Err(anyhow!("type is not integer").context(ctx())),
    }
}

/// Verifies alias constraints (currently no additional validation).
fn verify_alias(_world: &World, _ty: &Type, _value: &Alias) -> anyhow::Result<()> {
    // nothing to do

    Ok(())
}

// verify unique names?

/// Verifies a struct/pack/sequence member points at a known type.
fn verify_structmem(world: &World, ty: &Type, value: &StructMember) -> anyhow::Result<()> {
    let ctx = || format!("while verifying {}.{}", Typeref::from(ty), value.name);

    world
        .definitions
        .get(&value.ty)
        .context("member refers to unknown type")
        .with_context(ctx)?;

    Ok(())
}

/// Verifies that all strings in an iterator are unique.
fn verify_unique<'a>(mut iter: impl Iterator<Item = &'a String>) -> anyhow::Result<()> {
    if !iter.all_unique() {
        let s = iter.duplicates().join(",");

        bail!("all names must be unique: {s}");
    }

    Ok(())
}

/// Verifies that a `pack` has unique members and that each member is POD.
fn verify_pack(world: &World, ty: &Type, value: &Pack) -> anyhow::Result<()> {
    let ctx = || format!("while verifying pack {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    for m in value.members.iter() {
        verify_structmem(world, ty, m).with_context(ctx)?;
        verify_is_pod(world, m.ty).with_context(ctx)?;
    }

    Ok(())
}

/// Verifies enum invariants: unique names/values, unsigned integer base type, and valid default.
fn verify_enum(world: &World, ty: &Type, value: &Enum) -> anyhow::Result<()> {
    let ctx = || format!("while verifying enum {}", Typeref::from(ty));

    verify_unique(
        value
            .members
            .iter()
            .map(|x| &x.name)
            .chain(value.default.iter().map(|x| &x.name)),
    )
    .with_context(ctx)?;

    {
        let mut set = HashSet::<i64>::default();

        for enumerant in &value.members {
            if set.contains(&enumerant.value) {
                bail!(
                    "Duplicate enumerant value: {} -> {} at {}",
                    enumerant.name,
                    enumerant.value,
                    enumerant.defined_at
                );
            }

            set.insert(enumerant.value);
        }
    }

    let underlying = verify_is_int_of(world, value.underlying, Signedness::Unsigned)
        .context("checking underlying type")
        .with_context(ctx)?;

    for x in &value.members {
        underlying
            .verify_can_fit(x.value as i128)
            .with_context(|| format!("checking member {} at {}", x.name, x.defined_at))
            .with_context(ctx)?;
    }

    if let Some(default) = &value.default {
        underlying
            .verify_can_fit(default.value as i128)
            .with_context(|| {
                format!(
                    "checking default {} at {}",
                    default.name, default.defined_at
                )
            })
            .with_context(ctx)?;
    }

    Ok(())
}

/// Verifies bitfield invariants: unsigned underlying type, non-overlapping valid ranges, and member types.
fn verify_bitfld(world: &World, ty: &Type, value: &Bitfld) -> anyhow::Result<()> {
    let ctx = || format!("while verifying bitfld {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    let underlying = match &world.lookup(value.underlying).kind {
        TypeKind::Enum(enm) => verify_is_int_of(world, enm.underlying, Signedness::Unsigned),
        _ => verify_is_int_of(world, value.underlying, Signedness::Unsigned),
    }
    .with_context(ctx)?;

    let width = underlying.bits();
    let mut occupied: u128 = 0;

    for m in &value.members {
        verify_bitfield_member_type(world, m)
            .with_context(|| format!("checking member {} at {}", m.name, m.defined_at))
            .with_context(ctx)?;

        let start = *m.range.start();
        let end = *m.range.end();
        if start > end {
            bail!(
                "bitfield range start {start} greater than end {end} for member {}",
                m.name
            );
        }
        if end >= width {
            bail!(
                "bitfield range end {end} exceeds underlying width {width} for member {}",
                m.name
            );
        }

        let len = end - start + 1;
        let mask = ((1u128 << len) - 1) << start;
        if occupied & mask != 0 {
            bail!(
                "bitfield member {} overlaps with previous members in {}",
                m.name,
                Typeref::from(ty)
            );
        }
        occupied |= mask;
    }

    Ok(())
}

/// Verifies that a bitfield member's underlying type is integer-compatible.
fn verify_bitfield_member_type(world: &World, member: &BitfldMember) -> anyhow::Result<()> {
    let ty = world.lookup(member.underlying);
    let ctx = || format!("while checking {}", Typeref::from(ty));

    match &ty.kind {
        TypeKind::Alias(alias) => verify_bitfield_member_type(
            world,
            &BitfldMember {
                name: member.name.clone(),
                underlying: alias.other,
                range: member.range.clone(),
                defined_at: member.defined_at.clone(),
            },
        )
        .with_context(ctx),
        TypeKind::Enum(enm) => {
            verify_is_integer(world, enm.underlying).with_context(ctx)?;
            Ok(())
        }
        TypeKind::Primitive(p) if matches!(p.dtype, Datatype::Integer) => Ok(()),
        _ => Err(anyhow!("bitfield members must be primitive integers or enums").context(ctx())),
    }
}

/// Verifies that a variant has unique discriminants and a valid unsigned tag type.
fn verify_variant(world: &World, ty: &Type, value: &Variant) -> anyhow::Result<()> {
    let ctx = || format!("while verifying variant {}", Typeref::from(ty));

    let underlying = verify_is_int_of(world, value.discriminant, Signedness::Unsigned)
        .context("underlying type")
        .with_context(ctx)?;

    let mut seen = HashSet::new();
    let mut check_value = |val: u64, at: &SourceLocation| -> anyhow::Result<()> {
        if !seen.insert(val) {
            bail!("duplicate discriminant value {val}");
        }
        underlying
            .verify_can_fit(val as i128)
            .with_context(|| format!("checking discriminant {val} at {at}"))
    };

    for member in &value.members {
        check_value(member.value, &member.defined_at).with_context(ctx)?;
    }

    Ok(())
}

/// Verifies that a sequence has unique member names and valid member types.
fn verify_sequence(world: &World, ty: &Type, value: &Sequence) -> anyhow::Result<()> {
    let ctx = || format!("while verifying sequence {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    for member in &value.members {
        verify_structmem(world, ty, member).with_context(ctx)?;
    }

    Ok(())
}

/// Verifies that a dynamic array has a valid unsigned length type and non-`void` element type.
fn verify_dynarray(world: &World, ty: &Type, value: &DynamicArray) -> anyhow::Result<()> {
    let ctx = || format!("while verifying dynarray {}", Typeref::from(ty));

    verify_is_int_of(world, value.size_type, Signedness::Unsigned).with_context(ctx)?;
    let value_ty = world.lookup(value.value_type);
    if matches!(value_ty.kind, TypeKind::Void) {
        bail!("dynarray element type may not be void");
    }

    Ok(())
}

/// Verifies that a fixed array has a positive length and a POD element type.
fn verify_fixarray(world: &World, ty: &Type, value: &FixedArray) -> anyhow::Result<()> {
    let ctx = || format!("while verifying fixarray {}", Typeref::from(ty));

    if value.count == 0 {
        bail!("fixed arrays must have a positive element count");
    }

    verify_is_pod(world, value.value_type).with_context(ctx)?;

    Ok(())
}

/// Runs verification across every definition in the `World`.
fn verify(world: World) -> anyhow::Result<World> {
    for item in world.definitions.values() {
        match &item.kind {
            TypeKind::Alias(v) => verify_alias(&world, item, v)?,
            TypeKind::Pack(v) => verify_pack(&world, item, v)?,
            TypeKind::Enum(v) => verify_enum(&world, item, v)?,
            TypeKind::Bitfld(v) => verify_bitfld(&world, item, v)?,
            TypeKind::Variant(v) => verify_variant(&world, item, v)?,
            TypeKind::Sequence(v) => verify_sequence(&world, item, v)?,
            TypeKind::DynamicArray(v) => verify_dynarray(&world, item, v)?,
            TypeKind::FixedArray(v) => verify_fixarray(&world, item, v)?,
            TypeKind::Primitive(_) => {}
            TypeKind::Void => {}
        }
    }

    Ok(world)
}

/// Compiles an intermediate `Module` into a validated `World` for code generation.
pub fn compile(module: Module) -> anyhow::Result<World> {
    let mut allocator = TypeIDAllocator::new();

    let (mut name_to_id, mut defs) = {
        let names = [
            Primitive {
                width: BitWidth::W8,
                sign: Signedness::Signed,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W16,
                sign: Signedness::Signed,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W32,
                sign: Signedness::Signed,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W64,
                sign: Signedness::Signed,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W8,
                sign: Signedness::Unsigned,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W16,
                sign: Signedness::Unsigned,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W32,
                sign: Signedness::Unsigned,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W64,
                sign: Signedness::Unsigned,
                dtype: Datatype::Integer,
            },
            Primitive {
                width: BitWidth::W32,
                sign: Signedness::Signed,
                dtype: Datatype::Float,
            },
            Primitive {
                width: BitWidth::W64,
                sign: Signedness::Signed,
                dtype: Datatype::Float,
            },
        ];

        let builtin_defined_at = SourceLocation::new(
            SourceCode(std::sync::Arc::new("builtin".into())),
            Position { line: 0, column: 0 },
        );

        // Seed the world with builtin primitives/void so user definitions can refer to them.
        let names: Vec<_> = names
            .into_iter()
            .map(|x| {
                let tname =
                    TypeName::from_string(builtin_defined_at.clone(), x.to_string()).unwrap();
                (
                    x,
                    allocator.next(),
                    Type {
                        ident: tname,
                        defined_at: builtin_defined_at.clone(),
                        kind: TypeKind::Primitive(x),
                    },
                )
            })
            .collect();

        let mut name_to_id: HashMap<_, _> =
            names.iter().map(|x| (x.2.ident.clone(), x.1)).collect();

        let mut defs: HashMap<_, _> = names.into_iter().map(|x| (x.1, x.2)).collect();

        let void_name = TypeName::from_string(builtin_defined_at, "void").unwrap();
        let void_tid = allocator.next();

        name_to_id.insert(void_name.clone(), void_tid);
        defs.insert(
            void_tid,
            Type {
                ident: void_name,
                defined_at: SourceLocation::new(
                    SourceCode(std::sync::Arc::new("builtin".into())),
                    Position { line: 0, column: 0 },
                ),
                kind: TypeKind::Void,
            },
        );

        (name_to_id, defs)
    };

    for def in module.definitions.iter() {
        if let Some(existing) = name_to_id.get(&def.ident) {
            let prev = defs.get(existing).expect("type id without definition");
            bail!(
                "duplicate type name {} defined at {} (previously defined at {})",
                def.ident,
                def.defined_at,
                prev.defined_at
            );
        }

        let ident = allocator.next();
        name_to_id.insert(def.ident.clone(), ident);
    }

    let cs = CompileState { name_to_id };

    let converted = module
        .definitions
        .into_iter()
        .map(|item| convert(&cs, item))
        .collect::<anyhow::Result<Vec<_>>>()?;
    defs.extend(converted);

    let sorted = toposort(&defs)?;

    verify(World {
        definitions: defs,
        sorted,
        module_name: module.name,
    })
}

/// Returns all direct `TypeID` dependencies referenced by a type.
fn direct_dependencies(ty: &Type) -> Vec<TypeID> {
    match &ty.kind {
        TypeKind::Alias(alias) => vec![alias.other],
        TypeKind::Pack(pack) => pack.members.iter().map(|m| m.ty).collect(),
        TypeKind::Enum(enm) => vec![enm.underlying],
        TypeKind::Bitfld(bitfld) => {
            let mut deps: Vec<TypeID> = Vec::with_capacity(bitfld.members.len() + 1);
            deps.push(bitfld.underlying);
            deps.extend(bitfld.members.iter().map(|m| m.underlying));
            deps
        }
        TypeKind::Variant(variant) => {
            let mut deps: Vec<TypeID> = Vec::with_capacity(variant.members.len() + 1);
            deps.push(variant.discriminant);
            deps.extend(variant.members.iter().map(|m| m.ty));
            deps
        }
        TypeKind::Sequence(sequence) => sequence.members.iter().map(|m| m.ty).collect(),
        TypeKind::DynamicArray(dynamic_array) => {
            vec![dynamic_array.size_type, dynamic_array.value_type]
        }
        TypeKind::FixedArray(fixed_array) => vec![fixed_array.value_type],
        TypeKind::Primitive(_) | TypeKind::Void => Vec::new(),
    }
}

/// Finds an example dependency cycle, if one exists.
fn find_cycle(defs: &HashMap<TypeID, Type>) -> Option<Vec<TypeID>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Visiting,
        Visited,
    }

    /// DFS helper used to reconstruct a cycle path.
    fn dfs(
        node: TypeID,
        defs: &HashMap<TypeID, Type>,
        state: &mut HashMap<TypeID, State>,
        stack: &mut Vec<TypeID>,
    ) -> Option<Vec<TypeID>> {
        state.insert(node, State::Visiting);
        stack.push(node);

        let ty = defs.get(&node)?;
        for dep in direct_dependencies(ty) {
            if !defs.contains_key(&dep) {
                continue;
            }
            if matches!(state.get(&dep), Some(State::Visiting)) {
                let start = stack.iter().position(|&x| x == dep).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(dep);
                return Some(cycle);
            }
            if !matches!(state.get(&dep), Some(State::Visited)) {
                if let Some(found) = dfs(dep, defs, state, stack) {
                    return Some(found);
                }
            }
        }

        state.insert(node, State::Visited);
        stack.pop();
        None
    }

    let mut state = HashMap::new();
    let mut stack = Vec::new();

    for &id in defs.keys() {
        if state.contains_key(&id) {
            continue;
        }
        if let Some(cycle) = dfs(id, defs, &mut state, &mut stack) {
            return Some(cycle);
        }
    }

    None
}

/// Produces a stable topological ordering over all definitions.
fn toposort(defs: &HashMap<TypeID, Type>) -> anyhow::Result<Vec<TypeID>> {
    let mut indegree: HashMap<TypeID, usize> = HashMap::new();
    let mut dependents: HashMap<TypeID, Vec<TypeID>> = HashMap::new();

    let mut ids: Vec<_> = defs.keys().copied().collect();
    ids.sort();

    for &id in &ids {
        indegree.insert(id, 0);
        dependents.insert(id, Vec::new());
    }

    for &id in &ids {
        let ty = defs
            .get(&id)
            .expect("type id inserted into indegree without definition");

        let mut seen = HashSet::new();
        for dep in direct_dependencies(ty) {
            if !defs.contains_key(&dep) {
                continue;
            }
            if seen.insert(dep) {
                dependents.entry(dep).or_default().push(id);
                *indegree
                    .get_mut(&id)
                    .expect("indegree missing for previously inserted id") += 1;
            }
        }
    }

    for deps in dependents.values_mut() {
        deps.sort();
    }

    // Use a min-heap on TypeID to make the traversal deterministic across runs.
    let mut queue: BinaryHeap<Reverse<TypeID>> = indegree
        .iter()
        .filter_map(|(&id, &deg)| if deg == 0 { Some(Reverse(id)) } else { None })
        .collect();
    let mut order: Vec<TypeID> = Vec::with_capacity(indegree.len());

    while let Some(Reverse(id)) = queue.pop() {
        order.push(id);
        if let Some(nexts) = dependents.get(&id) {
            for &n in nexts {
                if let Some(d) = indegree.get_mut(&n) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push(Reverse(n));
                    }
                }
            }
        }
    }

    if order.len() < indegree.len() {
        if let Some(cycle) = find_cycle(defs) {
            let cycle_names: Vec<_> = cycle
                .into_iter()
                .filter_map(|id| defs.get(&id).map(|ty| ty.ident.to_string()))
                .collect();
            bail!(
                "cyclic type definitions detected: {}",
                cycle_names.join(" -> ")
            );
        } else {
            bail!("cyclic type definitions detected");
        }
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate;

    /// Test: overlapping bitfield ranges are rejected.
    #[test]
    fn bitfield_overlap_is_rejected() {
        let src = r#"
bits Bad : u8
- 0-2 a : u8
- 2-3 b : u8
"#;

        let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
        let err = compile(module).expect_err("compile should reject overlapping bit ranges");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("overlaps") || msg.contains("overlap"),
            "unexpected error message: {msg}"
        );
    }

    /// Test: dynamic arrays cannot have `void` element types.
    #[test]
    fn dynarray_void_element_is_rejected() {
        let src = r#"
dyn_array Bad : u8 * void
"#;

        let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
        let err = compile(module).expect_err("compile should reject void element type");
        let msg = format!("{err:#}");
        assert!(msg.contains("void"), "unexpected error message: {msg}");
    }

    /// Test: cyclic type references are rejected.
    #[test]
    fn cycles_are_rejected() {
        let src = r#"
alias A : B
alias B : A
"#;

        let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
        let err = compile(module).expect_err("compile should reject cycles");
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("cyclic"),
            "unexpected error message: {msg}"
        );
    }

    /// Test: direct self-references are rejected.
    #[test]
    fn self_references_are_rejected() {
        let src = r#"
alias A : A
"#;

        let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
        let err = compile(module).expect_err("compile should reject self reference");
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("cyclic"),
            "unexpected error message: {msg}"
        );
    }

    /// Test: topological sorting is deterministic and stable.
    #[test]
    fn topological_sort_is_stable() {
        let src = r#"
pack A
- a : u8

pack B
- b : u8

pack C
- a : A
- b : B
"#;

        let expected = vec!["A", "B", "C"];
        let mut seen_orders = vec![];

        for _ in 0..8 {
            let module = intermediate::Module::from_string("file".into(), src.to_string()).unwrap();
            let world = compile(module).expect("compile should succeed");
            let names: Vec<String> = world
                .iter()
                .filter(|(_, ty)| !matches!(ty.kind, TypeKind::Void | TypeKind::Primitive(_)))
                .map(|(_, ty)| ty.ident.to_string())
                .collect();

            assert_eq!(names, expected, "toposort should respect declaration order");
            seen_orders.push(names);
        }

        let first = seen_orders.first().expect("at least one order recorded");
        for order in seen_orders.iter().skip(1) {
            assert_eq!(
                order, first,
                "toposort should be deterministic across invocations"
            );
        }
    }

    /// Test: unknown type references are rejected.
    #[test]
    fn unknown_types_are_rejected() {
        let src = r#"
alias A : Missing
"#;

        let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
        let err = compile(module).expect_err("compile should reject unknown type names");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Missing"),
            "unexpected error message, wanted type name: {msg}"
        );
    }
}
