use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::BufRead,
    iter::Peekable,
    ops::RangeInclusive,
    rc::{Rc, Weak},
};
use thiserror::Error;

use crate::intermediate::{Module, Position};

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub ty: AType,
    pub defined_at: DefinedAt,
}

/// Plain-old-data aggregate with C-like layout.
#[derive(Debug)]
pub struct Pack {
    pub members: Vec<StructMember>,
}

#[derive(Debug)]
pub struct EnumMember {
    pub name: String,
    pub value: i64,
    pub defined_at: DefinedAt,
}

/// Enum with an explicit primitive underlying type.
#[derive(Debug)]
pub struct Enum {
    pub underlying: AType,
    pub members: Vec<EnumMember>,
    pub default: Option<EnumMember>,
}

#[derive(Debug)]
pub struct BitfldMember {
    pub name: String,
    pub underlying: AType,
    pub range: RangeInclusive<u32>,
    pub defined_at: DefinedAt,
}

/// Bitfield backed by an integer/enum type with named bit ranges.
#[derive(Debug)]
pub struct Bitfld {
    pub underlying: AType,
    pub members: Vec<BitfldMember>,
}

#[derive(Debug)]
pub struct VariantMember {
    pub ty: AType,
    pub value: u64,
    pub defined_at: Position,
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub discriminant: AType,
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
    pub other: AType,
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
    pub defined_at: DefinedAt,
    pub kind: TypeKind,
}

type AType = Rc<RefCell<Type>>;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct TypeName(Rc<String>);

#[derive(Debug)]
pub struct DefinedAt {
    module: Weak<RefCell<CompiledModule>>,
    position: Position,
}

pub struct CompiledModule {
    source_code: String,

    definitions: HashMap<TypeName, AType>,
}

pub fn compile(module: Module) -> CompiledModule {
    let definitions = HashMap::<TypeName, AType>::new();
    for item in module.definitions {
        // allocate a type

        let typename = TypeName(Rc::new(item.ident.clone()));

        definitions.insert(
            typename.clone(),
            Type {
                ident: todo!(),
                defined_at: todo!(),
                kind: todo!(),
            },
        )
    }

    todo!()
}
