use std::{collections::HashMap, io::BufRead, iter::Peekable, ops::RangeInclusive};
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

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct TypeName(std::rc::Rc<String>);

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct TypeID(u32);

#[derive(Debug)]
pub struct DefinedAt {
    module: std::rc::Rc<CompiledModuleInfo>,
    position: Position,
}

#[derive(Debug)]
pub struct CompiledModuleInfo {
    source_code: String,
}

pub struct World {
    definitions: HashMap<TypeID, Type>,
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
    name_to_id: HashMap<TypeName, TypeID>
}

pub fn convert(name_to_id: )

pub fn compile(module: Module) -> World {
    let shared_info = std::rc::Rc::new(CompiledModuleInfo {
        source_code: module.source,
    });

    let mut allocator = TypeIDAllocator::new();

    let name_to_id: HashMap<TypeName, TypeID> = module
        .definitions
        .iter()
        .map(|x| {
            let ident = allocator.next();

            let name = TypeName(std::rc::Rc::new(x.ident.clone()));
            (name, ident)
        })
        .collect();

    let defs: HashMap<_, _> = module
        .definitions
        .iter()
        .map(|item| {

            
            // what is our id?
            let this_id 

            let ident = allocator.next();

            let name = TypeName(std::rc::Rc::new(item.ident.clone()));

            let cell = Type {
                ident: name,
                defined_at: DefinedAt {
                    module: shared_info.clone(),
                    position: item.defined_at,
                },
                kind: TypeKind::Undefined,
            };

            (item.ident, (item, cell))
        })
        .collect();

    for def in defs {}

    ret
}
