use std::{collections::HashMap, ops::RangeInclusive};

use anyhow::Context;

use crate::intermediate;

use super::{
    Bitfld, BitfldMember, DynamicArray, FixedArray, Pack, Sequence, StructMember, Type, TypeID,
    TypeKind, Variant, VariantMember,
};

pub(super) struct TypeIDAllocator {
    last: u32,
}

impl TypeIDAllocator {
    /// Creates a new allocator.
    pub(super) fn new() -> Self {
        Self { last: 0 }
    }

    /// Allocates the next available `TypeID`.
    pub(super) fn next(&mut self) -> TypeID {
        let r = TypeID(self.last);
        self.last += 1;
        r
    }
}

pub(super) struct CompileState {
    pub(super) name_to_id: HashMap<intermediate::TypeName, TypeID>,
}

impl CompileState {
    /// Resolves an intermediate `TypeName` to a compiled `TypeID`.
    fn lookup(&self, tname: &intermediate::TypeName) -> anyhow::Result<TypeID> {
        let Some(x) = self.name_to_id.get(tname) else {
            anyhow::bail!("unknown type name `{tname}`");
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
pub(super) fn convert(
    state: &CompileState,
    ty: intermediate::Type,
) -> anyhow::Result<(TypeID, Type)> {
    let this_id = state.lookup(&ty.ident)?;

    let new_kind = match ty.kind {
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
        intermediate::TypeKind::Enum(enm) => TypeKind::Enum(super::Enum {
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
