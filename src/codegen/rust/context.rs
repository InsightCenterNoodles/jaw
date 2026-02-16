use crate::{
    GlobalOptions,
    codegen::Sink,
    compile::{Primitive, Type, TypeID, TypeKind, World},
};

use anyhow::Result;

use std::collections::HashMap;

pub struct RustContext<'a> {
    world: &'a World,
    options: &'a GlobalOptions,
    names: HashMap<TypeID, String>,
}

impl<'a> RustContext<'a> {
    /// Builds a codegen context (name mapping and options).
    pub fn new(world: &'a World, options: &'a GlobalOptions) -> Result<Self> {
        let mut names = HashMap::new();
        for (id, ty) in world.iter() {
            names.insert(id, sanitize(ty.ident.to_string()));
        }
        Ok(Self {
            world,
            options,
            names,
        })
    }

    pub fn lookup(&self, tname: TypeID) -> &Type {
        self.world.lookup(tname)
    }

    /// Returns the generated Rust identifier for a `TypeID`.
    pub fn name_of(&self, id: TypeID) -> String {
        self.names
            .get(&id)
            .expect("missing generated name for type id")
            .clone()
    }

    /// Iterates types in dependency order.
    pub fn types(&'a self) -> impl Iterator<Item = (TypeID, &'a Type)> + 'a {
        self.world.iter()
    }

    /// Returns the primitive for a type if it is directly primitive.
    pub fn direct_primitive(&self, id: TypeID) -> Option<Primitive> {
        match &self.world.lookup(id).kind {
            TypeKind::Primitive(p) => Some(*p),
            TypeKind::Const(c) => self.direct_primitive(c.ty),
            _ => None,
        }
    }

    /// Returns the primitive for a type following aliases and enums, if any.
    pub fn underlying_primitive(&self, id: TypeID) -> Option<Primitive> {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(p) => Some(*p),
            TypeKind::Enum(e) => self.underlying_primitive(e.underlying),
            TypeKind::Const(c) => self.underlying_primitive(c.ty),
            _ => None,
        }
    }

    /// Returns whether a read-view type requires a lifetime parameter.
    pub fn view_needs_lifetime(&self, id: TypeID) -> bool {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(_) | TypeKind::Void | TypeKind::Enum(_) => false,
            TypeKind::Const(c) => self.view_needs_lifetime(c.ty),
            TypeKind::Bitfld(bitfld) => bitfld
                .members
                .iter()
                .any(|m| self.view_needs_lifetime(m.underlying)),
            TypeKind::Pack(_) => false,
            TypeKind::Sequence(seq) => seq.members.iter().any(|m| self.view_needs_lifetime(m.ty)),
            TypeKind::Variant(variant) => variant
                .members
                .iter()
                .any(|m| self.view_needs_lifetime(m.ty)),
            TypeKind::DynamicArray(_) => true,
            TypeKind::FixedArray(_) => true,
        }
    }

    /// Returns whether an array can use the bulk read/write path, and if so, the element kind.
    pub fn can_bulk_array(&self, id: TypeID) -> Option<&TypeKind> {
        let ty = self.world.lookup(id);

        match &ty.kind {
            TypeKind::DynamicArray(dynamic_array) => {
                let elem = dynamic_array.value_type;
                match &self.world.lookup(elem).kind {
                    TypeKind::Primitive(_) | TypeKind::Pack(_) => {
                        Some(&self.world.lookup(elem).kind)
                    }
                    TypeKind::Const(c) => match &self.world.lookup(c.ty).kind {
                        TypeKind::Primitive(_) => Some(&self.world.lookup(c.ty).kind),
                        _ => None,
                    },
                    _ => None,
                }
            }
            TypeKind::FixedArray(fixed_array) => {
                let elem = fixed_array.value_type;
                match &self.world.lookup(elem).kind {
                    TypeKind::Primitive(_) | TypeKind::Pack(_) => {
                        Some(&self.world.lookup(elem).kind)
                    }
                    TypeKind::Const(c) => match &self.world.lookup(c.ty).kind {
                        TypeKind::Primitive(_) => Some(&self.world.lookup(c.ty).kind),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Returns the Rust view type spelling for a `TypeID` using `lifetime`.
    pub fn rust_view_type(&self, id: TypeID) -> String {
        let ty = self.world.lookup(id);
        let name = self.name_of(id);

        let out = match &ty.kind {
            TypeKind::Primitive(_) => name,
            TypeKind::Void => "void".into(),
            TypeKind::Enum(_) | TypeKind::Bitfld(_) | TypeKind::Pack(_) | TypeKind::Const(_) => {
                name
            }
            TypeKind::Sequence(_)
            | TypeKind::Variant(_)
            | TypeKind::DynamicArray(_)
            | TypeKind::FixedArray(_) => {
                format!("{name}View")
            }
        };

        out
    }

    /// Returns the write-module declaration type for `id`, preserving alias names.
    pub fn rust_write_decl_type(&self, id: TypeID, lifetime: &str) -> Result<String> {
        let ty = self.world.lookup(id);
        let out = match &ty.kind {
            TypeKind::Primitive(_) => self.name_of(id),
            TypeKind::Void => "()".into(),
            TypeKind::Enum(_) | TypeKind::Bitfld(_) | TypeKind::Pack(_) | TypeKind::Const(_) => {
                self.name_of(id)
            }
            TypeKind::Sequence(_) | TypeKind::Variant(_) => {
                let name = self.name_of(id);
                if self.view_needs_lifetime(id) {
                    format!("{name}View<{lifetime}>")
                } else {
                    name
                }
            }
            TypeKind::DynamicArray(_) | TypeKind::FixedArray(_) => {
                format!("{}View<{lifetime}>", self.name_of(id))
            }
        };
        Ok(out)
    }

    /// Emits an optional byte-size guard for array parsing when enabled.
    pub fn insert_optional_size_check(
        &self,
        dest: &mut impl Sink,
        count_var_name: &str,
        value_ty_name: &str,
    ) {
        let Some(byte_limit) = self.options.guard_array_size else {
            return;
        };
        dest.wln(&format!(
            "let byte_count: u128 = ({} as u128)",
            count_var_name
        ));
        dest.wln(&format!(
            "    .checked_mul(std::mem::size_of::<{}>() as u128)",
            value_ty_name
        ));
        dest.wln("    .ok_or_else(|| invalid_data(\"array too large\"))?;");
        dest.wln(&format!(
            "if byte_count > {}u128 {{ return Err(invalid_data(\"array too large\")); }}",
            byte_limit
        ));
    }
}

/// Sanitizes DSL identifiers into valid Rust identifiers.
fn sanitize<S: AsRef<str>>(s: S) -> String {
    let raw = s.as_ref();
    let mut out = String::with_capacity(raw.len());
    for (i, ch) in raw.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_';
        if !valid {
            out.push('_');
            continue;
        }
        if i == 0 && ch.is_ascii_digit() {
            out.push('_');
        }
        out.push(ch);
    }
    if out.is_empty() { "_t".into() } else { out }
}

// /// Maps a DSL primitive to a Rust primitive type name.
// fn map_primitive(p: Primitive) -> &'static str {
//     let s = match (p.dtype, p.sign, p.width) {
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => "u8",
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => "u16",
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => "u32",
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => "u64",
//         (Datatype::Integer, Signedness::Signed, BitWidth::W8) => "i8",
//         (Datatype::Integer, Signedness::Signed, BitWidth::W16) => "i16",
//         (Datatype::Integer, Signedness::Signed, BitWidth::W32) => "i32",
//         (Datatype::Integer, Signedness::Signed, BitWidth::W64) => "i64",
//         (Datatype::Float, _, BitWidth::W32) => "f32",
//         (Datatype::Float, _, BitWidth::W64) => "f64",
//         // We should be handling every type we know about
//         _ => panic!("unsupported primitive type"),
//     };
//     s
// }
