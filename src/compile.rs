use anyhow::{Context, anyhow, bail};
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    pub default: Option<VariantMember>,
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
    fn bits(&self) -> u32 {
        match self.width {
            BitWidth::W8 => 8,
            BitWidth::W16 => 16,
            BitWidth::W32 => 32,
            BitWidth::W64 => 64,
        }
    }

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

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct TypeID(u32);

pub struct World {
    definitions: HashMap<TypeID, Type>,
    sorted: Vec<TypeID>,
}

impl World {
    fn lookup(&self, tname: TypeID) -> &Type {
        self.definitions.get(&tname).unwrap()
    }
}

struct TypeIDAllocator {
    last: u32,
}

impl TypeIDAllocator {
    fn new() -> Self {
        Self { last: 0 }
    }
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
    fn lookup(&self, tname: &intermediate::TypeName) -> TypeID {
        *match self.name_to_id.get(tname) {
            Some(x) => x,
            None => {
                panic!("internal error, missing type name: {tname}")
            }
        }
    }
}

fn string_to_range(range: String) -> RangeInclusive<u32> {
    if let Some((a, b)) = range.split_once('-') {
        RangeInclusive::new(a.parse().unwrap(), b.parse().unwrap())
    } else {
        let v = range.parse().unwrap();
        RangeInclusive::new(v, v)
    }
}

fn convert(state: &CompileState, ty: intermediate::Type) -> (TypeID, Type) {
    let this_id = state.lookup(&ty.ident);

    let new_kind = match ty.kind {
        intermediate::TypeKind::Alias(alias) => TypeKind::Alias(Alias {
            other: state.lookup(&alias.other),
        }),
        intermediate::TypeKind::Pack(pack) => TypeKind::Pack(Pack {
            members: pack
                .members
                .into_iter()
                .map(|x| StructMember {
                    name: x.name,
                    ty: state.lookup(&x.ty),
                    defined_at: x.defined_at,
                })
                .collect(),
        }),
        intermediate::TypeKind::Enum(enm) => TypeKind::Enum(Enum {
            underlying: state.lookup(&enm.ty),
            members: enm.members,
            default: enm.default,
        }),
        intermediate::TypeKind::Bitfld(bitfld) => TypeKind::Bitfld(Bitfld {
            underlying: state.lookup(&bitfld.ty),
            members: bitfld
                .members
                .into_iter()
                .map(|x| BitfldMember {
                    name: x.name,
                    underlying: state.lookup(&x.ty),
                    range: string_to_range(x.range),
                    defined_at: x.defined_at,
                })
                .collect(),
        }),
        intermediate::TypeKind::Variant(variant) => {
            let convert_member = |member: intermediate::VariantMember| VariantMember {
                ty: state.lookup(&member.ty),
                value: member.value,
                defined_at: member.defined_at,
            };

            TypeKind::Variant(Variant {
                discriminant: state.lookup(&variant.ty),
                members: variant.members.into_iter().map(convert_member).collect(),
                default: variant.default.map(convert_member),
            })
        }
        intermediate::TypeKind::Sequence(sequence) => TypeKind::Sequence(Sequence {
            members: sequence
                .members
                .into_iter()
                .map(|x| StructMember {
                    name: x.name,
                    ty: state.lookup(&x.ty),
                    defined_at: x.defined_at,
                })
                .collect(),
        }),
        intermediate::TypeKind::DynamicArray(dynamic_array) => {
            TypeKind::DynamicArray(DynamicArray {
                size_type: state.lookup(&dynamic_array.size_type),
                value_type: state.lookup(&dynamic_array.value_type),
            })
        }
        intermediate::TypeKind::FixedArray(fixed_array) => TypeKind::FixedArray(FixedArray {
            count: fixed_array.count,
            value_type: state.lookup(&fixed_array.value_type),
        }),
    };

    (
        this_id,
        Type {
            ident: ty.ident,
            defined_at: ty.defined_at,
            kind: new_kind,
        },
    )
}

// MARK: Verify

#[derive(Debug)]
struct Typeref {
    name: TypeName,
    at: SourceLocation,
}

impl From<&Type> for Typeref {
    fn from(value: &Type) -> Self {
        Self {
            name: value.ident.clone(),
            at: value.defined_at.clone(),
        }
    }
}

impl Display for Typeref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Symbol {} {}", self.name, self.at)
    }
}

#[derive(Debug, Error)]
enum NotPODError {
    #[error("Type {0} is not POD")]
    TypeNotPOD(Typeref),
}

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

/// Asset that a type is an integer with a certain sign
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

fn verify_is_integer(world: &World, tid: TypeID) -> anyhow::Result<Primitive> {
    let ty = world.lookup(tid);

    let ctx = || format!("while checking {}", Typeref::from(ty));

    match &ty.kind {
        TypeKind::Alias(alias) => verify_is_integer(world, alias.other).with_context(ctx),
        TypeKind::Primitive(x) if matches!(x.dtype, Datatype::Integer) => Ok(*x),
        _ => Err(anyhow!("type is not integer").context(ctx())),
    }
}

fn verify_alias(_world: &World, _ty: &Type, _value: &Alias) -> anyhow::Result<()> {
    // nothing to do

    Ok(())
}

// verify unique names?

fn verify_structmem(world: &World, ty: &Type, value: &StructMember) -> anyhow::Result<()> {
    let ctx = || format!("while verifying {}.{}", Typeref::from(ty), value.name);

    world
        .definitions
        .get(&value.ty)
        .context("member refers to unknown type")
        .with_context(ctx)?;

    Ok(())
}

fn verify_unique<'a>(mut iter: impl Iterator<Item = &'a String>) -> anyhow::Result<()> {
    if !iter.all_unique() {
        let s = iter.duplicates().join(",");

        bail!("all names must be unique: {s}");
    }

    Ok(())
}

fn verify_pack(world: &World, ty: &Type, value: &Pack) -> anyhow::Result<()> {
    let ctx = || format!("while verifying pack {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    for m in value.members.iter() {
        verify_structmem(world, ty, m).with_context(ctx)?;
        verify_is_pod(world, m.ty).with_context(ctx)?;
    }

    Ok(())
}
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
fn verify_bitfld(world: &World, ty: &Type, value: &Bitfld) -> anyhow::Result<()> {
    let ctx = || format!("while verifying bitfld {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    let underlying = match &world.lookup(value.underlying).kind {
        TypeKind::Enum(enm) => verify_is_int_of(world, enm.underlying, Signedness::Unsigned),
        _ => verify_is_int_of(world, value.underlying, Signedness::Unsigned),
    }
    .with_context(ctx)?;

    let width = underlying.bits();

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
    }

    Ok(())
}

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

    if let Some(default) = &value.default {
        check_value(default.value, &default.defined_at).with_context(ctx)?;
    }

    Ok(())
}
fn verify_sequence(world: &World, ty: &Type, value: &Sequence) -> anyhow::Result<()> {
    let ctx = || format!("while verifying sequence {}", Typeref::from(ty));

    verify_unique(value.members.iter().map(|x| &x.name)).with_context(ctx)?;

    for member in &value.members {
        verify_structmem(world, ty, member).with_context(ctx)?;
    }

    Ok(())
}
fn verify_dynarray(world: &World, ty: &Type, value: &DynamicArray) -> anyhow::Result<()> {
    let ctx = || format!("while verifying dynarray {}", Typeref::from(ty));

    verify_is_int_of(world, value.size_type, Signedness::Unsigned).with_context(ctx)?;

    Ok(())
}
fn verify_fixarray(world: &World, ty: &Type, value: &FixedArray) -> anyhow::Result<()> {
    let ctx = || format!("while verifying fixarray {}", Typeref::from(ty));

    if value.count == 0 {
        bail!("fixed arrays must have a positive element count");
    }

    verify_is_pod(world, value.value_type).with_context(ctx)?;

    Ok(())
}

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

        let names: Vec<_> = names
            .into_iter()
            .map(|x| {
                let tname = TypeName::from(x.to_string());
                (
                    x.clone(),
                    allocator.next(),
                    Type {
                        ident: tname,
                        defined_at: SourceLocation::new(
                            SourceCode(std::sync::Arc::new("builtin".into())),
                            Position { line: 0, column: 0 },
                        ),
                        kind: TypeKind::Primitive(x),
                    },
                )
            })
            .collect();

        let mut name_to_id: HashMap<_, _> =
            names.iter().map(|x| (x.2.ident.clone(), x.1)).collect();

        let mut defs: HashMap<_, _> = names.into_iter().map(|x| (x.1, x.2)).collect();

        let void_name: TypeName = "void".into();
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

    name_to_id.extend(module.definitions.iter().map(|x| {
        let ident = allocator.next();

        let name = x.ident.clone();
        (name, ident)
    }));

    let cs = CompileState { name_to_id };

    defs.extend(
        module
            .definitions
            .into_iter()
            .map(|item| convert(&cs, item)),
    );

    let sorted = toposort(&defs);

    verify(World {
        definitions: defs,
        sorted,
    })
}

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
            let mut deps: Vec<TypeID> = Vec::with_capacity(variant.members.len() + 2);
            deps.push(variant.discriminant);
            deps.extend(variant.members.iter().map(|m| m.ty));
            if let Some(default) = &variant.default {
                deps.push(default.ty);
            }
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

fn toposort(defs: &HashMap<TypeID, Type>) -> Vec<TypeID> {
    let mut indegree: HashMap<TypeID, usize> = HashMap::new();
    let mut dependents: HashMap<TypeID, Vec<TypeID>> = HashMap::new();

    for &id in defs.keys() {
        indegree.entry(id).or_insert(0);
        dependents.entry(id).or_default();
    }

    for (&id, ty) in defs {
        let mut seen = HashSet::new();
        for dep in direct_dependencies(ty) {
            if dep == id || !defs.contains_key(&dep) {
                continue;
            }
            if seen.insert(dep) {
                dependents.entry(dep).or_default().push(id);
                *indegree.entry(id).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<TypeID> = indegree
        .iter()
        .filter_map(|(&id, &deg)| if deg == 0 { Some(id) } else { None })
        .collect();
    let mut order: Vec<TypeID> = Vec::with_capacity(indegree.len());

    while let Some(id) = queue.pop_front() {
        order.push(id);
        if let Some(nexts) = dependents.get(&id) {
            for &n in nexts {
                if let Some(d) = indegree.get_mut(&n) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push_back(n);
                    }
                }
            }
        }
    }

    if order.len() < indegree.len() {
        let already: HashSet<_> = order.iter().copied().collect();
        for &id in indegree.keys() {
            if !already.contains(&id) {
                order.push(id);
            }
        }
    }

    order
}
