mod convert;
mod toposort;
mod types;
mod verify;

#[cfg(test)]
mod tests;

pub use types::{
    BitWidth, Bitfld, BitfldMember, Const, Datatype, DynamicArray, Enum, FixedArray, Pack,
    Primitive, PrimitiveLiteral, Sequence, Signedness, StructMember, Type, TypeID, TypeKind,
    Variant, VariantMember, World,
};

use anyhow::bail;
use std::collections::HashMap;

use crate::intermediate::{Module, Position, SourceCode, SourceLocation, TypeName};

use convert::{CompileState, TypeIDAllocator, convert};
use toposort::toposort;
use verify::verify;

/// Compiles an intermediate `Module` into a validated `World` for code generation.
pub fn compile(module: Module) -> anyhow::Result<World> {
    let mut allocator = TypeIDAllocator::new();

    let (mut name_to_id, mut name_to_location, mut defs) = {
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

        let mut name_to_location: HashMap<_, _> = names
            .iter()
            .map(|x| (x.2.ident.clone(), x.2.defined_at.clone()))
            .collect();

        let mut defs: HashMap<_, _> = names.into_iter().map(|x| (x.1, x.2)).collect();

        let bool_name = TypeName::from_string(builtin_defined_at.clone(), "bool").unwrap();
        let bool_tid = allocator.next();
        name_to_id.insert(bool_name.clone(), bool_tid);
        name_to_location.insert(bool_name.clone(), builtin_defined_at.clone());
        defs.insert(
            bool_tid,
            Type {
                ident: bool_name,
                defined_at: builtin_defined_at.clone(),
                kind: TypeKind::Bool,
            },
        );

        let void_name = TypeName::from_string(builtin_defined_at, "void").unwrap();
        let void_tid = allocator.next();
        let void_defined_at = SourceLocation::new(
            SourceCode(std::sync::Arc::new("builtin".into())),
            Position { line: 0, column: 0 },
        );

        name_to_id.insert(void_name.clone(), void_tid);
        name_to_location.insert(void_name.clone(), void_defined_at.clone());
        defs.insert(
            void_tid,
            Type {
                ident: void_name,
                defined_at: void_defined_at,
                kind: TypeKind::Void,
            },
        );

        (name_to_id, name_to_location, defs)
    };

    for def in module.definitions.iter() {
        if def.ident.as_str() == "WireBool" {
            bail!(
                "type name {} at {} is reserved for generated boolean storage",
                def.ident,
                def.defined_at
            );
        }
        if let Some(prev) = name_to_location.get(&def.ident) {
            bail!(
                "duplicate type name {} defined at {} (previously defined at {})",
                def.ident,
                def.defined_at,
                prev
            );
        }

        let ident = allocator.next();
        name_to_id.insert(def.ident.clone(), ident);
        name_to_location.insert(def.ident.clone(), def.defined_at.clone());
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
