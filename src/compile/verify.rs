use std::fmt::Display;

use anyhow::{Context, anyhow, bail};
use itertools::Itertools;
use thiserror::Error;

use crate::intermediate::{SourceLocation, TypeName};

use super::{
    Bitfld, BitfldMember, Datatype, DynamicArray, Enum, FixedArray, Pack, Primitive, Sequence,
    Signedness, StructMember, Type, TypeID, TypeKind, Variant, World,
};

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
        TypeKind::Primitive(x) if matches!(x.dtype, Datatype::Integer) => Ok(*x),
        _ => Err(anyhow!("type is not integer").context(ctx())),
    }
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
        let mut set = std::collections::HashSet::<i64>::default();

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

    let mut seen = std::collections::HashSet::new();
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
pub(super) fn verify(world: World) -> anyhow::Result<World> {
    for item in world.definitions.values() {
        match &item.kind {
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
