use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};

use crate::compile::{
    BitWidth, Datatype, Primitive, Signedness, Type, TypeID, TypeKind, VariantMember, World,
};

use super::*;

/// Emits a Rust module for the given compiled `World`.
pub fn emit(world: &World, global: &GlobalOptions, out: &mut Outfile) -> Result<()> {
    let ctx = RustContext::new(world, global)?;

    emit_preamble(out);
    out.newline();

    emit_module(&ctx, out, RustFlavor::Read)?;
    out.newline();
    emit_module(&ctx, out, RustFlavor::Write)?;

    Ok(())
}

#[derive(Clone, Copy)]
enum RustFlavor {
    Read,
    Write,
}

impl RustFlavor {
    /// Returns the module name used for this flavor (`read` vs `write`).
    fn module_name(self) -> &'static str {
        match self {
            RustFlavor::Read => "read",
            RustFlavor::Write => "write",
        }
    }
}

struct RustContext<'a> {
    world: &'a World,
    options: &'a GlobalOptions,
    names: HashMap<TypeID, String>,
}

impl<'a> RustContext<'a> {
    /// Builds a codegen context (name mapping and options).
    fn new(world: &'a World, options: &'a GlobalOptions) -> Result<Self> {
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

    /// Returns the generated Rust identifier for a `TypeID`.
    fn name_of(&self, id: TypeID) -> String {
        self.names
            .get(&id)
            .expect("missing generated name for type id")
            .clone()
    }

    /// Iterates types in dependency order.
    fn types(&'a self) -> impl Iterator<Item = (TypeID, &'a Type)> + 'a {
        self.world.iter()
    }

    /// Resolves aliases transitively.
    fn resolve_alias(&self, id: TypeID) -> TypeID {
        let mut cur = id;
        loop {
            let ty = self.world.lookup(cur);
            match &ty.kind {
                TypeKind::Alias(a) => cur = a.other,
                _ => return cur,
            }
        }
    }

    /// Returns the primitive for a type if it is directly primitive (after alias resolution).
    fn direct_primitive(&self, id: TypeID) -> Option<Primitive> {
        let resolved = self.resolve_alias(id);
        match &self.world.lookup(resolved).kind {
            TypeKind::Primitive(p) => Some(*p),
            _ => None,
        }
    }

    /// Returns the primitive for a type following aliases and enums, if any.
    fn underlying_primitive(&self, id: TypeID) -> Option<Primitive> {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(p) => Some(*p),
            TypeKind::Alias(a) => self.underlying_primitive(a.other),
            TypeKind::Enum(e) => self.underlying_primitive(e.underlying),
            _ => None,
        }
    }

    // fn is_pod(&self, id: TypeID) -> bool {
    //     let ty = self.world.lookup(id);
    //     match &ty.kind {
    //         TypeKind::Alias(alias) => self.is_pod(alias.other),
    //         TypeKind::Pack(_) => true,
    //         TypeKind::Enum(_) => true,
    //         TypeKind::Bitfld(_) => true,
    //         TypeKind::Variant(_) => false,
    //         TypeKind::Sequence(_) => false,
    //         TypeKind::DynamicArray(_) => false,
    //         TypeKind::FixedArray(fixed) => self.is_pod(fixed.value_type),
    //         TypeKind::Primitive(_) => true,
    //         TypeKind::Void => false,
    //     }
    // }

    /// Returns whether a read-view type requires a lifetime parameter.
    fn view_needs_lifetime(&self, id: TypeID) -> bool {
        let resolved = self.resolve_alias(id);
        let ty = self.world.lookup(resolved);
        match &ty.kind {
            TypeKind::Primitive(_) | TypeKind::Void | TypeKind::Enum(_) => false,
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
            TypeKind::Alias(alias) => self.view_needs_lifetime(alias.other),
        }
    }

    /// Returns the owned Rust type spelling for a `TypeID`.
    fn rust_type(&self, id: TypeID) -> Result<String> {
        let resolved = self.resolve_alias(id);
        let ty = self.world.lookup(resolved);
        let out = match &ty.kind {
            TypeKind::Primitive(p) => map_primitive(*p)?.to_string(),
            TypeKind::Void => "()".into(),
            _ => self.name_of(resolved),
        };
        Ok(out)
    }

    /// Returns the read-module declaration type for `id`, preserving alias names.
    fn rust_read_decl_type(&self, id: TypeID) -> Result<String> {
        let ty = self.world.lookup(id);
        let out = match &ty.kind {
            TypeKind::Primitive(p) => map_primitive(*p)?.to_string(),
            TypeKind::Void => "()".into(),
            TypeKind::Alias(_) => self.name_of(id),
            _ => self.name_of(self.resolve_alias(id)),
        };
        Ok(out)
    }

    /// Returns whether an array can use the bulk read/write path, and if so, the element kind.
    fn can_bulk_array(&self, id: TypeID) -> Option<&TypeKind> {
        let ty = self.world.lookup(id);

        match &ty.kind {
            TypeKind::DynamicArray(dynamic_array) => {
                if matches!(
                    self.world.lookup(dynamic_array.value_type).kind,
                    TypeKind::Alias(_)
                ) {
                    return None;
                }
                let elem = self.resolve_alias(dynamic_array.value_type);
                match &self.world.lookup(elem).kind {
                    TypeKind::Primitive(_) | TypeKind::Pack(_) => {
                        Some(&self.world.lookup(elem).kind)
                    }
                    _ => None,
                }
            }
            TypeKind::FixedArray(fixed_array) => {
                if matches!(
                    self.world.lookup(fixed_array.value_type).kind,
                    TypeKind::Alias(_)
                ) {
                    return None;
                }
                let elem = self.resolve_alias(fixed_array.value_type);
                match &self.world.lookup(elem).kind {
                    TypeKind::Primitive(_) | TypeKind::Pack(_) => {
                        Some(&self.world.lookup(elem).kind)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Returns the Rust view type spelling for a `TypeID` using `lifetime`.
    fn rust_view_type(&self, id: TypeID, lifetime: &str) -> Result<String> {
        let resolved = self.resolve_alias(id);
        let ty = self.world.lookup(resolved);
        let name = self.name_of(resolved);
        let needs_lt = self.view_needs_lifetime(resolved);
        let lt_suffix = if needs_lt {
            format!("<{lifetime}>")
        } else {
            String::new()
        };

        let out = match &ty.kind {
            TypeKind::Primitive(p) => map_primitive(*p)?.to_string(),
            TypeKind::Void => "()".into(),
            TypeKind::Enum(_) => name,
            TypeKind::Bitfld(_)
            | TypeKind::Pack(_)
            | TypeKind::Sequence(_)
            | TypeKind::Variant(_) => {
                format!("{name}{lt_suffix}")
            }
            TypeKind::DynamicArray(_) | TypeKind::FixedArray(_) => {
                format!("{name}View<{lifetime}>")
            }
            TypeKind::Alias(_) => unreachable!("aliases resolved earlier"),
        };
        Ok(out)
    }

    /// Returns the write-module declaration type for `id`, preserving alias names.
    fn rust_write_decl_type(&self, id: TypeID, lifetime: &str) -> Result<String> {
        let ty = self.world.lookup(id);
        let out = match &ty.kind {
            TypeKind::Primitive(p) => map_primitive(*p)?.to_string(),
            TypeKind::Void => "()".into(),
            TypeKind::Alias(_) => {
                let name = self.name_of(id);
                if self.view_needs_lifetime(id) {
                    format!("{name}<{lifetime}>")
                } else {
                    name
                }
            }
            TypeKind::Enum(_) | TypeKind::Bitfld(_) | TypeKind::Pack(_) => {
                self.name_of(self.resolve_alias(id))
            }
            TypeKind::Sequence(_) | TypeKind::Variant(_) => {
                let resolved = self.resolve_alias(id);
                let name = self.name_of(resolved);
                if self.view_needs_lifetime(id) {
                    format!("{name}<{lifetime}>")
                } else {
                    name
                }
            }
            TypeKind::DynamicArray(_) | TypeKind::FixedArray(_) => {
                format!("{}View<{lifetime}>", self.name_of(self.resolve_alias(id)))
            }
        };
        Ok(out)
    }

    /// Emits an optional byte-size guard for array parsing when enabled.
    fn insert_optional_size_check(
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

/// Emits top-level imports and shared helpers for generated Rust.
fn emit_preamble(out: &mut impl Sink) {
    out.wln("// Generated by jaw. Do not edit.");
    out.wln("use std::io::{self, Read, Write};");
    out.wln("use std::convert::TryInto;");

    out.newline();

    out.wln("fn invalid_data(msg: impl Into<String>) -> io::Error");
    {
        let mut idt = out.indent();
        idt.wln("io::Error::new(io::ErrorKind::InvalidData, msg.into())");
    }
    out.newline();

    for (fname, ty) in [
        ("read_u8", "u8"),
        ("read_i8", "i8"),
        ("read_u16", "u16"),
        ("read_i16", "i16"),
        ("read_u32", "u32"),
        ("read_i32", "i32"),
        ("read_u64", "u64"),
        ("read_i64", "i64"),
        ("read_f32", "f32"),
        ("read_f64", "f64"),
    ] {
        out.wln("#[inline]");
        out.wln(&format!(
            "fn {fname}<R: Read>(reader: &mut R) -> io::Result<{ty}>"
        ));
        {
            let mut idt = out.indent();
            idt.wln(&format!(
                "let mut buf = [0u8; std::mem::size_of::<{ty}>()];"
            ));
            idt.wln("reader.read_exact(&mut buf)?;");
            idt.wln(&format!("Ok({ty}::from_le_bytes(buf))"));
        }
    }
    out.newline();

    for (fname, ty) in [
        ("write_u8", "u8"),
        ("write_i8", "i8"),
        ("write_u16", "u16"),
        ("write_i16", "i16"),
        ("write_u32", "u32"),
        ("write_i32", "i32"),
        ("write_u64", "u64"),
        ("write_i64", "i64"),
        ("write_f32", "f32"),
        ("write_f64", "f64"),
    ] {
        out.wln("#[inline]");
        out.wln(&format!(
            "fn {fname}<W: Write>(writer: &mut W, value: {ty}) -> io::Result<()>"
        ));
        {
            let mut idt = out.indent();
            idt.wln("writer.write_all(&value.to_le_bytes())");
        }
    }
    out.newline();
}

/// Emits a `read` or `write` module (types + impls) for the compiled `World`.
fn emit_module(ctx: &RustContext, out: &mut impl Sink, flavor: RustFlavor) -> Result<()> {
    out.wln(&format!("pub mod {}", flavor.module_name()));
    {
        let mut module = out.indent();
        module.wln("use super::*;");
        module.newline();

        for (id, ty) in ctx.types() {
            match flavor {
                RustFlavor::Read => emit_read_type_definition(ctx, &mut module, id, ty)?,
                RustFlavor::Write => emit_write_type_definition(ctx, &mut module, id, ty)?,
            }
        }

        for (id, ty) in ctx.types() {
            match flavor {
                RustFlavor::Read => emit_read_impl(ctx, &mut module, id, ty)?,
                RustFlavor::Write => emit_write_impl(ctx, &mut module, id, ty)?,
            }
        }
    }
    Ok(())
}

/// Emits owned Rust type definitions used by the `read` module.
fn emit_read_type_definition(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> Result<()> {
    let name = ctx.name_of(id);
    match &ty.kind {
        TypeKind::Alias(alias) => {
            let target_ty = ctx.rust_read_decl_type(alias.other)?;
            out.wln("#[derive(Debug, Clone, PartialEq)]");
            out.wln(&format!("pub struct {name}(pub {target_ty});"));
            out.newline();
        }
        TypeKind::Enum(enm) => {
            let base = ctx.rust_type(enm.underlying)?;
            out.wln(&format!("#[repr({base})]"));
            out.wln("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
            out.wln(&format!("pub enum {name}"));
            {
                let mut idt = out.indent();
                for m in &enm.members {
                    idt.wln(&format!("{} = {},", sanitize(&m.name), m.value));
                }
                if let Some(default) = &enm.default {
                    idt.wln(&format!("{} = {},", sanitize(&default.name), default.value));
                }
            }
            out.newline();
        }
        TypeKind::Bitfld(bitfld) => {
            out.wln("#[derive(Debug, Clone, PartialEq)]");
            out.wln(&format!("pub struct {name}"));
            {
                let mut idt = out.indent();
                for m in &bitfld.members {
                    let field_ty = ctx.rust_type(m.underlying)?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }
            out.newline();
        }
        TypeKind::Pack(pack) => {
            out.wln("#[repr(C, packed(1))]");
            out.wln(&format!("pub struct {name}"));
            {
                let mut idt = out.indent();
                for m in &pack.members {
                    let field_ty = ctx.rust_type(m.ty)?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }

            out.wln(&format!("impl std::fmt::Debug for {name}"));
            {
                let mut idt = out.indent();
                idt.wln("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result");
                {
                    let mut idt = idt.indent();

                    for item in &pack.members {
                        idt.wln(&format!("let {0} = self.{0};", item.name));
                    }

                    idt.wln(&format!("f.debug_struct(\"{name}\")"));

                    for item in &pack.members {
                        idt.wln(&format!(".field(\"{0}\", &{0})", item.name));
                    }

                    idt.wln(".finish()");
                }
            }

            out.wln(&format!("impl Clone for {name}"));
            {
                let mut idt = out.indent();
                idt.wln("fn clone(&self) -> Self");
                {
                    let mut idt = idt.indent();

                    idt.wln("Self");
                    {
                        let mut idt = idt.indent();
                        for item in &pack.members {
                            idt.wln(&format!("{0}: self.{0},", item.name));
                        }
                    }
                }
            }

            out.wln(&format!("impl PartialEq for {name}"));
            {
                let mut idt = out.indent();
                idt.wln("fn eq(&self, other: &Self) -> bool");
                {
                    let mut idt = idt.indent();

                    let mut mem_check = vec![];

                    for item in &pack.members {
                        mem_check.push(format!("self.{0} == other.{0}", item.name));
                    }

                    idt.wln(&(String::from("return ") + &mem_check.join("&&")));
                }
            }

            out.wln(&format!("impl Copy for {name} {{ }}"));

            out.wln(&format!("unsafe impl bytemuck::Zeroable for {name} {{ }}"));
            out.wln(&format!("unsafe impl bytemuck::Pod for {name} {{ }}"));
            out.wln(&format!("impl Default for {name}"));
            {
                let mut idt = out.indent();
                idt.wln("fn default() -> Self");
                {
                    let mut body = idt.indent();
                    body.wln(&format!("<{name} as bytemuck::Zeroable>::zeroed()"));
                }
            }

            out.newline();
        }
        TypeKind::Sequence(seq) => {
            out.wln("#[derive(Debug, Clone, PartialEq)]");
            out.wln(&format!("pub struct {name}"));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    let field_ty = ctx.rust_read_decl_type(m.ty)?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }
            out.newline();
        }
        TypeKind::Variant(variant) => {
            out.wln("#[derive(Debug, Clone, PartialEq)]");
            out.wln(&format!("pub enum {name}"));
            {
                let mut idt = out.indent();
                for (idx, m) in variant.members.iter().enumerate() {
                    let case_name = variant_case_name(ctx, m, idx)?;
                    let payload = if matches!(
                        ctx.world.lookup(ctx.resolve_alias(m.ty)).kind,
                        TypeKind::Void
                    ) {
                        None
                    } else {
                        Some(ctx.rust_read_decl_type(m.ty)?)
                    };

                    if let Some(mty) = payload {
                        idt.wln(&format!("{case_name}({mty}),"));
                    } else {
                        idt.wln(&format!("{case_name},"));
                    }
                }
            }
            out.newline();
        }
        TypeKind::DynamicArray(arr) => {
            let elem = ctx.rust_read_decl_type(arr.value_type)?;
            out.wln(&format!("pub type {name} = Vec<{elem}>;"));
            out.newline();
        }
        TypeKind::FixedArray(arr) => {
            let elem = ctx.rust_read_decl_type(arr.value_type)?;
            out.wln(&format!("pub type {name} = [{elem}; {}];", arr.count));
            out.newline();
        }
        TypeKind::Primitive(_) | TypeKind::Void => {}
    }

    Ok(())
}

/// Emits Rust view type definitions used by the `write` module.
fn emit_write_type_definition(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> Result<()> {
    let name = ctx.name_of(id);
    match &ty.kind {
        TypeKind::Alias(alias) => {
            let target_ty = ctx.rust_write_decl_type(alias.other, "'a")?;
            let needs_lt = ctx.view_needs_lifetime(alias.other);
            out.wln("#[derive(Debug, Clone, Copy, PartialEq)]");
            if needs_lt {
                out.wln("#[allow(unused_lifetimes)]");
                out.wln(&format!("pub struct {name}<'a>(pub {target_ty});"));
            } else {
                out.wln(&format!("pub struct {name}(pub {target_ty});"));
            }
            out.newline();
        }
        TypeKind::Enum(enm) => {
            let base = ctx.rust_type(enm.underlying)?;
            out.wln(&format!("#[repr({base})]"));
            out.wln("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
            out.wln(&format!("pub enum {name}"));
            {
                let mut idt = out.indent();
                for m in &enm.members {
                    idt.wln(&format!("{} = {},", sanitize(&m.name), m.value));
                }
                if let Some(default) = &enm.default {
                    idt.wln(&format!("{} = {},", sanitize(&default.name), default.value));
                }
            }
            out.newline();
        }
        TypeKind::Bitfld(bitfld) => {
            out.wln("#[derive(Debug, Clone, Copy, PartialEq)]");
            out.wln(&format!("pub struct {name}"));
            {
                let mut idt = out.indent();
                for m in &bitfld.members {
                    let field_ty = ctx.rust_type(m.underlying)?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }
            out.newline();
        }
        TypeKind::Pack(pack) => {
            out.wln("#[derive(Debug, Default, Clone, Copy, PartialEq)]");
            out.wln("#[repr(C, packed(1))]");
            out.wln(&format!("pub struct {name}"));
            {
                let mut idt = out.indent();
                for m in &pack.members {
                    let field_ty = ctx.rust_type(m.ty)?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }
            out.wln(&format!("unsafe impl bytemuck::Zeroable for {name} {{ }}"));
            out.wln(&format!("unsafe impl bytemuck::Pod for {name} {{ }}"));
            out.newline();
        }
        TypeKind::Sequence(seq) => {
            let needs_lt = ctx.view_needs_lifetime(id);
            let lt = if needs_lt { "<'a>" } else { "" };
            out.wln("#[derive(Debug, Clone, Copy, PartialEq)]");
            out.wln(&format!("pub struct {name}{lt}"));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    let field_ty = ctx.rust_write_decl_type(m.ty, "'a")?;
                    idt.wln(&format!("pub {}: {},", sanitize(&m.name), field_ty));
                }
            }
            out.newline();
        }
        TypeKind::Variant(variant) => {
            let has_ref_payload = variant.members.iter().any(|m| {
                !matches!(
                    ctx.world.lookup(ctx.resolve_alias(m.ty)).kind,
                    TypeKind::Void
                )
            });
            let needs_lt = has_ref_payload || ctx.view_needs_lifetime(id);
            let lt = if needs_lt { "<'a>" } else { "" };
            out.wln("#[derive(Debug, Clone, Copy, PartialEq)]");
            out.wln(&format!("pub enum {name}{lt}"));
            {
                let mut idt = out.indent();
                for (idx, m) in variant.members.iter().enumerate() {
                    let case_name = variant_case_name(ctx, m, idx)?;
                    let payload = if matches!(
                        ctx.world.lookup(ctx.resolve_alias(m.ty)).kind,
                        TypeKind::Void
                    ) {
                        None
                    } else {
                        Some(ctx.rust_write_decl_type(m.ty, "'a")?)
                    };

                    if let Some(mty) = payload {
                        idt.wln(&format!("{case_name}(&'a {mty}),"));
                    } else {
                        idt.wln(&format!("{case_name},"));
                    }
                }
            }
            out.newline();
        }
        TypeKind::DynamicArray(arr) => {
            let elem_view = ctx.rust_write_decl_type(arr.value_type, "'a")?;
            out.wln(&format!("pub type {name}View<'a> = &'a [{elem_view}];"));
            out.newline();
        }
        TypeKind::FixedArray(arr) => {
            let elem_view = ctx.rust_read_decl_type(arr.value_type)?;
            out.wln(&format!("pub type {name} = [{elem_view}; {}];", arr.count));
            let elem_view = ctx.rust_write_decl_type(arr.value_type, "'a")?;
            out.wln(&format!("pub type {name}View<'a> = &'a [{elem_view}];"));
            out.newline();
        }
        TypeKind::Primitive(_) | TypeKind::Void => {}
    }

    Ok(())
}

/// Emits `read_*` functions for a type.
fn emit_read_impl(ctx: &RustContext, out: &mut impl Sink, id: TypeID, ty: &Type) -> Result<()> {
    let name = ctx.name_of(id);
    match &ty.kind {
        TypeKind::Primitive(_) => {}
        TypeKind::Void => {}
        TypeKind::Alias(alias) => {
            let target_ty = ctx.rust_read_decl_type(id)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{target_ty}>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("let inner = {}?;", read_expr(ctx, alias.other, "reader")?));
                idt.wln(&format!("Ok({name}(inner))"));
            }
            out.newline();
        }
        TypeKind::Enum(enm) => {
            let base_read = read_primitive_method(ctx, enm.underlying)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{name}>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("let raw = {base_read}(reader)?;"));
                idt.wln("match raw");
                {
                    let mut mtch = idt.indent();
                    for m in &enm.members {
                        mtch.wln(&format!(
                            "{} => Ok({}::{}),",
                            m.value,
                            name,
                            sanitize(&m.name)
                        ));
                    }
                    if let Some(default) = &enm.default {
                        mtch.wln(&format!("_ => Ok({}::{}),", name, sanitize(&default.name)));
                    } else {
                        mtch.wln(&format!(
                            "_ => Err(invalid_data(format!(\"invalid discriminant for {name}: {{raw}}\"))),"
                        ));
                    }
                }
            }
            out.newline();
        }
        TypeKind::Bitfld(bitfld) => {
            let base_read = read_primitive_method(ctx, bitfld.underlying)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{name}>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("let raw = {base_read}(reader)? as u128;"));
                for m in &bitfld.members {
                    let start = m.range.start();
                    let end = m.range.end();
                    let width = end - start + 1;
                    let mask = (1u128 << width) - 1;
                    let field = sanitize(&m.name);
                    let field_ty = ctx.world.lookup(m.underlying);
                    let assign_expr = match &field_ty.kind {
                        TypeKind::Enum(enm) => {
                            let mut s = String::new();
                            s.push_str(&format!(
                                "let {field}_raw = ((raw >> {start}) & 0x{mask:X}) as {};\n",
                                ctx.rust_type(enm.underlying)?
                            ));
                            s.push_str("let ");
                            s.push_str(&field);
                            s.push_str(" = match ");
                            s.push_str(&format!("{field}_raw"));
                            s.push_str(" {\n");
                            for mem in &enm.members {
                                s.push_str(&format!(
                                    "    {val} => {enm_name}::{variant},\n",
                                    val = mem.value,
                                    enm_name = ctx.name_of(m.underlying),
                                    variant = sanitize(&mem.name)
                                ));
                            }
                            if let Some(default) = &enm.default {
                                s.push_str(&format!(
                                    "    _ => {enm_name}::{variant},\n",
                                    enm_name = ctx.name_of(m.underlying),
                                    variant = sanitize(&default.name)
                                ));
                            } else {
                                s.push_str(
                                    "    _ => return Err(invalid_data(\"invalid discriminant in bitfield\")),\n",
                                );
                            }
                            s.push_str("};");
                            s
                        }
                        _ => format!(
                            "let {field} = (((raw >> {start}) & 0x{mask:X}) as {}) as {};",
                            ctx.rust_type(m.underlying)?,
                            ctx.rust_type(m.underlying)?
                        ),
                    };
                    for line in assign_expr.lines() {
                        idt.wln(line);
                    }
                }
                idt.wln(&format!("Ok({name}"));
                {
                    let mut args = idt.indent();
                    for m in &bitfld.members {
                        let field = sanitize(&m.name);
                        args.wln(&format!("{field},"));
                    }
                }
                idt.wln(")");
            }
            out.newline();
        }
        TypeKind::Pack(_) => {
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{name}>"
            ));
            {
                let mut idt = out.indent();

                idt.wln("// Safety: Bytes will be overwritten anyway");
                idt.wln("#[allow(invalid_value)]");
                idt.wln("let mut tmp = unsafe { std::mem::MaybeUninit::uninit().assume_init() };");
                idt.wln("reader.read_exact(bytemuck::bytes_of_mut(&mut tmp))?;");
                idt.wln("Ok(tmp)");
            }
            out.newline();
        }
        TypeKind::Sequence(seq) => {
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{name}>"
            ));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    let expr = read_expr(ctx, m.ty, "reader")?;
                    idt.wln(&format!(
                        "let {} = {expr}?;",
                        sanitize(&m.name),
                        expr = expr
                    ));
                }
                idt.wln(&format!("Ok({name}"));
                {
                    let mut body = idt.indent();
                    for m in &seq.members {
                        let f = sanitize(&m.name);
                        body.wln(&format!("{f},"));
                    }
                }
                idt.wln(")");
            }
            out.newline();
        }
        TypeKind::Variant(variant) => {
            let disc_read = read_primitive_method(ctx, variant.discriminant)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<{name}>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("let tag = {disc_read}(reader)?;"));
                idt.wln("match tag");
                {
                    let mut mtch = idt.indent();
                    for (idx, m) in variant.members.iter().enumerate() {
                        let case_name = variant_case_name(ctx, m, idx)?;
                        mtch.wln(&format!("{} => ", m.value));
                        {
                            let mut body = mtch.indent();

                            let mty = if matches!(
                                ctx.world.lookup(ctx.resolve_alias(m.ty)).kind,
                                TypeKind::Void
                            ) {
                                "()".to_string()
                            } else {
                                ctx.rust_type(m.ty)?
                            };

                            if mty == "()" {
                                body.wln(&format!("Ok({name}::{case_name})"));
                            } else {
                                let expr = read_expr(ctx, m.ty, "reader")?;
                                body.wln(&format!("let payload = {expr}?;"));
                                body.wln(&format!("Ok({name}::{case_name}(payload))"));
                            }
                        }
                        mtch.wln(",");
                    }
                    mtch.wln(&format!(
                        "_ => Err(invalid_data(format!(\"unknown tag for {name}: {{tag}}\"))),"
                    ));
                }
            }
            out.newline();
        }
        TypeKind::DynamicArray(arr) => {
            let elem_name = ctx.rust_read_decl_type(arr.value_type)?;
            let size_read = read_primitive_method(ctx, arr.size_type)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<Vec<{elem_name}>>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("let count_raw = {size_read}(reader)?;"));
                idt.wln("let count: usize = count_raw.try_into().map_err(|_| invalid_data(\"array length too large\"))?;");

                ctx.insert_optional_size_check(&mut idt, "count", &elem_name);

                match ctx.can_bulk_array(id) {
                    Some(TypeKind::Primitive(x)) if x.is_u8() => {
                        idt.wln("let mut out = vec![Default::default(); count];");
                        idt.wln("reader.read_exact(&mut out)?;");
                    }
                    Some(_) => {
                        idt.wln("let mut out = vec![Default::default(); count];");
                        idt.wln("reader.read_exact(bytemuck::cast_slice_mut(&mut out))?;");
                    }
                    _ => {
                        idt.wln("let mut out = Vec::with_capacity(count);");
                        idt.wln("for _ in 0..count");
                        {
                            let mut body = idt.indent();
                            let expr = read_expr(ctx, arr.value_type, "reader")?;
                            body.wln(&format!("out.push({expr}?);"));
                        }
                    }
                }
                idt.wln("Ok(out)");
            }
            out.newline();
        }
        TypeKind::FixedArray(arr) => {
            let elem_name = ctx.rust_read_decl_type(arr.value_type)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn read_{name}<R: Read>(reader: &mut R) -> io::Result<[{}; {}]>",
                elem_name, arr.count
            ));

            {
                let mut idt = out.indent();

                idt.wln(&format!(
                    "let mut out : [{}; {}] = Default::default();",
                    elem_name, arr.count
                ));

                match ctx.can_bulk_array(id) {
                    Some(TypeKind::Primitive(x)) if x.is_u8() => {
                        idt.wln("reader.read_exact(&mut out)?;");
                    }
                    Some(_) => {
                        idt.wln("reader.read_exact(bytemuck::cast_slice_mut(&mut out))?;");
                    }
                    _ => {
                        idt.wln("for x in &mut out");
                        {
                            let mut body = idt.indent();
                            let expr = read_expr(ctx, arr.value_type, "reader")?;
                            body.wln(&format!("*x = {expr}?;"));
                        }
                    }
                }

                idt.wln("Ok(out)");
            }

            out.newline();
        }
    }

    Ok(())
}

/// Emits `write_*` functions for a type.
fn emit_write_impl(ctx: &RustContext, out: &mut impl Sink, id: TypeID, ty: &Type) -> Result<()> {
    let name = ctx.name_of(id);
    match &ty.kind {
        TypeKind::Primitive(_) => {}
        TypeKind::Void => {}
        TypeKind::Alias(alias) => {
            let target_view = if ctx.view_needs_lifetime(id) {
                format!("{name}<'_>")
            } else {
                name.clone()
            };
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{target_view}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                let expr = if ctx.direct_primitive(alias.other).is_some() {
                    "value.0"
                } else {
                    "&value.0"
                };
                write_value(ctx, &mut idt, alias.other, expr)?;
                idt.wln("Ok(())");
            }
            out.newline();
        }
        TypeKind::Enum(enm) => {
            let write_fn = write_primitive_method(ctx, enm.underlying)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{name}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{}(writer, *value as _)", write_fn));
            }
            out.newline();
        }
        TypeKind::Bitfld(bitfld) => {
            let write_fn = write_primitive_method(ctx, bitfld.underlying)?;
            let view_ty = ctx.rust_view_type(id, "'_")?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{view_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln("let mut raw: u128 = 0;");
                for m in &bitfld.members {
                    let start = m.range.start();
                    let end = m.range.end();
                    let width = end - start + 1;
                    let mask = (1u128 << width) - 1;
                    let fname = sanitize(&m.name);
                    idt.wln(&format!(
                        "raw |= ((value.{fname} as u128) & 0x{mask:X}) << {start};"
                    ));
                }
                idt.wln(&format!(
                    "{}(writer, raw as {})",
                    write_fn,
                    ctx.rust_type(bitfld.underlying)?
                ));
            }
            out.newline();
        }
        TypeKind::Pack(_) => {
            let view_ty = ctx.rust_view_type(id, "'_")?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{view_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln("writer.write_all(bytemuck::bytes_of(value))");
            }
            out.newline();
        }
        TypeKind::Sequence(seq) => {
            let view_ty = ctx.rust_view_type(id, "'_")?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{view_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    write_value(
                        ctx,
                        &mut idt,
                        m.ty,
                        &format!("&value.{}", sanitize(&m.name)),
                    )?;
                }
                idt.wln("Ok(())");
            }
            out.newline();
        }
        TypeKind::Variant(variant) => {
            let write_tag = write_primitive_method(ctx, variant.discriminant)?;
            let view_ty = ctx.rust_view_type(id, "'_")?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, value: &{view_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln("match value");
                {
                    let mut mtch = idt.indent();
                    for (idx, m) in variant.members.iter().enumerate() {
                        let case_name = variant_case_name(ctx, m, idx)?;
                        let is_void = matches!(
                            ctx.world.lookup(ctx.resolve_alias(m.ty)).kind,
                            TypeKind::Void
                        );

                        if is_void {
                            mtch.wln(&format!("{name}::{case_name} =>"));
                            {
                                let mut body = mtch.indent();
                                body.wln(&format!("{}(writer, {})?;", write_tag, m.value));
                                body.wln("Ok(())");
                            }
                            mtch.wln(",");
                        } else {
                            mtch.wln(&format!("{name}::{case_name}(inner) =>"));
                            {
                                let mut body = mtch.indent();
                                body.wln(&format!("{}(writer, {})?;", write_tag, m.value));
                                write_value(ctx, &mut body, m.ty, "inner")?;
                                body.wln("Ok(())");
                            }
                            mtch.wln(",");
                        }
                    }
                }
            }
            out.newline();
        }
        TypeKind::DynamicArray(arr) => {
            let values_ty = ctx.rust_view_type(id, "'_")?;
            let size_write = write_primitive_method(ctx, arr.size_type)?;
            let max_len = max_len_for_size(ctx, arr.size_type)?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, values: &{values_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln("let len = values.len();");
                idt.wln(&format!(
                    "let max_len: usize = {}usize;",
                    max_len.min(usize::MAX as u128)
                ));
                idt.wln(
                    "// Fail instead of truncating if the vector does not fit in the count type.",
                );
                idt.wln("if len > max_len { return Err(invalid_data(\"array length too large to encode\")); }");
                idt.wln("let count: u64 = len.try_into().map_err(|_| invalid_data(\"array length too large to encode\"))?;");
                idt.wln(&format!("{}(writer, count as _)?;", size_write));
                if ctx.direct_primitive(arr.value_type).is_some() {
                    idt.wln("writer.write_all(bytemuck::cast_slice(values))?;");
                } else {
                    idt.wln("for v in *values");
                    {
                        let mut body = idt.indent();
                        write_value(ctx, &mut body, arr.value_type, "v")?;
                    }
                }
                idt.wln("Ok(())");
            }
            out.newline();
        }
        TypeKind::FixedArray(arr) => {
            let values_ty = ctx.rust_view_type(id, "'_")?;
            out.wln("#[allow(non_snake_case)]");
            out.wln(&format!(
                "pub fn write_{name}<W: Write>(writer: &mut W, values: &{values_ty}) -> io::Result<()>"
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!(
                    "if values.len() != {} {{ return Err(invalid_data(\"unexpected fixed array length\")); }}",
                    arr.count
                ));
                if ctx.direct_primitive(arr.value_type).is_some() {
                    idt.wln("writer.write_all(bytemuck::cast_slice(values))?;");
                } else {
                    idt.wln("for v in *values");
                    {
                        let mut body = idt.indent();
                        write_value(ctx, &mut body, arr.value_type, "v")?;
                    }
                }
                idt.wln("Ok(())");
            }
            out.newline();
        }
    }

    Ok(())
}

/// Builds a Rust expression string to read a value of `id` from `reader_ident`.
fn read_expr(ctx: &RustContext, id: TypeID, reader_ident: &str) -> Result<String> {
    let ty = ctx.world.lookup(id);
    let expr = match &ty.kind {
        TypeKind::Primitive(p) => {
            let method = read_primitive_method_direct(*p)?;
            format!("{method}({reader_ident})")
        }
        TypeKind::Void => "Ok(())".into(),
        TypeKind::Alias(_) => format!("read_{}({reader_ident})", ctx.name_of(id)),
        _ => format!(
            "read_{}({reader_ident})",
            ctx.name_of(ctx.resolve_alias(id))
        ),
    };
    Ok(expr)
}

/// Emits Rust statements that write `value_expr` of type `id` into `writer`.
fn write_value(ctx: &RustContext, out: &mut impl Sink, id: TypeID, value_expr: &str) -> Result<()> {
    let ty = ctx.world.lookup(id);
    match &ty.kind {
        TypeKind::Primitive(p) => {
            let method = write_primitive_method_direct(*p)?;
            out.wln(&format!("{method}(writer, {value_expr})?;"));
        }
        TypeKind::Void => {}
        TypeKind::Alias(_) => {
            out.wln(&format!("write_{}(writer, {value_expr})?;", ctx.name_of(id)));
        }
        TypeKind::Enum(enm) => {
            let method = write_primitive_method(ctx, enm.underlying)?;
            out.wln(&format!("{method}(writer, {value_expr} as _)?;"));
        }
        TypeKind::DynamicArray(_) | TypeKind::FixedArray(_) => {
            out.wln(&format!(
                "write_{}(writer, {value_expr})?;",
                ctx.name_of(id)
            ));
        }
        _ => {
            out.wln(&format!(
                "write_{}(writer, {value_expr})?;",
                ctx.name_of(id)
            ));
        }
    }
    Ok(())
}

// Keep variant arm names stable and unique even when payload types repeat.
/// Computes a stable, unique variant case name.
fn variant_case_name(ctx: &RustContext, m: &VariantMember, _idx: usize) -> Result<String> {
    let base = ctx.name_of(ctx.resolve_alias(m.ty));
    Ok(sanitize(format!("{base}_{}", m.value)))
}

// Shared bound for dynamic array length encoding, based on the declared counter type.
/// Returns the maximum encodable array length for a given size-counter type.
fn max_len_for_size(ctx: &RustContext, id: TypeID) -> Result<u128> {
    let prim = ctx
        .underlying_primitive(id)
        .ok_or_else(|| anyhow!("expected primitive-compatible type"))?;
    match (prim.dtype, prim.sign, prim.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => Ok(u8::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => Ok(u16::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => Ok(u32::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => Ok(u64::MAX as u128),
        _ => bail!("array size type must be an unsigned integer"),
    }
}

/// Returns the reader method name for a primitive-compatible type.
fn read_primitive_method(ctx: &RustContext, id: TypeID) -> Result<&'static str> {
    let p = ctx
        .underlying_primitive(id)
        .ok_or_else(|| anyhow::anyhow!("expected primitive-compatible type"))?;
    read_primitive_method_direct(p)
}

/// Returns the writer method name for a primitive-compatible type.
fn write_primitive_method(ctx: &RustContext, id: TypeID) -> Result<&'static str> {
    let p = ctx
        .underlying_primitive(id)
        .ok_or_else(|| anyhow::anyhow!("expected primitive-compatible type"))?;
    write_primitive_method_direct(p)
}

/// Returns the reader method name for a specific primitive.
fn read_primitive_method_direct(p: Primitive) -> Result<&'static str> {
    match (p.dtype, p.sign, p.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => Ok("read_u8"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => Ok("read_u16"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => Ok("read_u32"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => Ok("read_u64"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W8) => Ok("read_i8"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W16) => Ok("read_i16"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W32) => Ok("read_i32"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W64) => Ok("read_i64"),
        (Datatype::Float, _, BitWidth::W32) => Ok("read_f32"),
        (Datatype::Float, _, BitWidth::W64) => Ok("read_f64"),
        _ => bail!("unsupported primitive type"),
    }
}

/// Returns the writer method name for a specific primitive.
fn write_primitive_method_direct(p: Primitive) -> Result<&'static str> {
    match (p.dtype, p.sign, p.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => Ok("write_u8"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => Ok("write_u16"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => Ok("write_u32"),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => Ok("write_u64"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W8) => Ok("write_i8"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W16) => Ok("write_i16"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W32) => Ok("write_i32"),
        (Datatype::Integer, Signedness::Signed, BitWidth::W64) => Ok("write_i64"),
        (Datatype::Float, _, BitWidth::W32) => Ok("write_f32"),
        (Datatype::Float, _, BitWidth::W64) => Ok("write_f64"),
        _ => bail!("unsupported primitive type"),
    }
}

/// Maps a DSL primitive to a Rust primitive type name.
fn map_primitive(p: Primitive) -> Result<&'static str> {
    let s = match (p.dtype, p.sign, p.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => "u8",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => "u16",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => "u32",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => "u64",
        (Datatype::Integer, Signedness::Signed, BitWidth::W8) => "i8",
        (Datatype::Integer, Signedness::Signed, BitWidth::W16) => "i16",
        (Datatype::Integer, Signedness::Signed, BitWidth::W32) => "i32",
        (Datatype::Integer, Signedness::Signed, BitWidth::W64) => "i64",
        (Datatype::Float, _, BitWidth::W32) => "f32",
        (Datatype::Float, _, BitWidth::W64) => "f64",
        _ => bail!("unsupported primitive type"),
    };
    Ok(s)
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
