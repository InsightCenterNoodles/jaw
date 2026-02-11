mod convert;
mod toposort;
mod types;
mod verify;

#[cfg(test)]
mod tests;

pub use types::{
    Alias, BitWidth, Bitfld, BitfldMember, Datatype, DynamicArray, Enum, FixedArray, Pack,
    Primitive, Sequence, Signedness, StructMember, Type, TypeID, TypeKind, Variant, VariantMember,
    World,
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
