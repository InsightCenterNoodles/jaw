use crate::compile::{StructMember, TypeID, TypeKind, World};

pub(super) fn fixed_wire_size(world: &World, id: TypeID) -> Option<u64> {
    let members = |values: &[StructMember]| {
        values.iter().try_fold(0u64, |sum, member| {
            sum.checked_add(fixed_wire_size(world, member.ty)?)
        })
    };
    match &world.lookup(id).kind {
        TypeKind::Bool => Some(1),
        TypeKind::Primitive(value) => Some(u64::from(value.bits() / 8)),
        TypeKind::Const(value) => fixed_wire_size(world, value.ty),
        TypeKind::Enum(value) => fixed_wire_size(world, value.underlying),
        TypeKind::Bitfld(value) => fixed_wire_size(world, value.underlying),
        TypeKind::Pack(value) => members(&value.members),
        TypeKind::Sequence(value) => members(&value.members),
        TypeKind::FixedArray(value) => {
            fixed_wire_size(world, value.value_type)?.checked_mul(value.count)
        }
        TypeKind::Variant(value) => {
            let tag = fixed_wire_size(world, value.discriminant)?;
            let mut sizes = value.members.iter().map(|member| {
                if matches!(world.lookup(member.ty).kind, TypeKind::Void) {
                    Some(0)
                } else {
                    fixed_wire_size(world, member.ty)
                }
            });
            let payload = sizes.next()??;
            sizes
                .all(|size| size == Some(payload))
                .then(|| tag.checked_add(payload))
                .flatten()
        }
        TypeKind::DynamicArray(_) => None,
        TypeKind::Void => Some(0),
    }
}

pub(super) fn minimum_wire_size(world: &World, id: TypeID) -> Option<u64> {
    let members = |values: &[StructMember]| {
        values.iter().try_fold(0u64, |sum, member| {
            sum.checked_add(minimum_wire_size(world, member.ty)?)
        })
    };
    match &world.lookup(id).kind {
        TypeKind::Bool => Some(1),
        TypeKind::Primitive(value) => Some(u64::from(value.bits() / 8)),
        TypeKind::Const(value) => minimum_wire_size(world, value.ty),
        TypeKind::Enum(value) => minimum_wire_size(world, value.underlying),
        TypeKind::Bitfld(value) => minimum_wire_size(world, value.underlying),
        TypeKind::Pack(value) => members(&value.members),
        TypeKind::Sequence(value) => members(&value.members),
        TypeKind::FixedArray(value) => {
            minimum_wire_size(world, value.value_type)?.checked_mul(value.count)
        }
        TypeKind::DynamicArray(value) => minimum_wire_size(world, value.size_type),
        TypeKind::Variant(value) => {
            let tag = minimum_wire_size(world, value.discriminant)?;
            let payload = value
                .members
                .iter()
                .map(|member| minimum_wire_size(world, member.ty))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .min()?;
            tag.checked_add(payload)
        }
        TypeKind::Void => Some(0),
    }
}
