use std::collections::HashMap;

use anyhow::{Result, bail};

use std::io::Write;

use crate::compile::{BitWidth, Datatype, Primitive, Signedness, Type, TypeID, TypeKind, World};

use super::*;

pub fn emit(world: &World, out: &mut Outfile) -> anyhow::Result<()> {
    let ctx = CppContext::new(world)?;

    emit_preamble(out, &ctx.namespace)?;

    {
        let mut ns = out.indent();
        emit_helpers(&mut ns);

        // Type declarations
        for (id, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            emit_type_def(&ctx, &mut ns, id, ty)?;
        }

        ns.newline();

        // Forward decls for read/write
        for (_, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            emit_forward_decls(&ctx, &mut ns, ty);
        }

        ns.newline();

        // Read/write implementations
        for (id, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            emit_rw_impl(&ctx, &mut ns, id, ty)?;
        }
    }

    Ok(())
}

struct CppContext<'a> {
    world: &'a World,
    names: HashMap<TypeID, String>,
    namespace: String,
}

impl<'a> CppContext<'a> {
    fn new(world: &'a World) -> anyhow::Result<Self> {
        let mut names = HashMap::new();
        for (id, ty) in world.iter() {
            names.insert(id, sanitize(ty.ident.to_string()));
        }
        Ok(Self {
            world,
            names,
            namespace: sanitize(world.module_name()),
        })
    }

    fn name_of(&self, id: TypeID) -> &str {
        self.names
            .get(&id)
            .expect("missing generated name for type id")
    }

    fn cpp_type(&self, id: TypeID) -> anyhow::Result<String> {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(p) => Ok(map_primitive(*p)?),
            TypeKind::Void => Ok("std::monostate".into()),
            _ => Ok(self.name_of(id).to_string()),
        }
    }

    fn underlying_primitive(&self, id: TypeID) -> anyhow::Result<Primitive> {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(p) => Ok(*p),
            TypeKind::Alias(a) => self.underlying_primitive(a.other),
            TypeKind::Enum(e) => self.underlying_primitive(e.underlying),
            _ => bail!("expected primitive-compatible type for {}", ty.ident),
        }
    }

    fn types(&'a self) -> impl Iterator<Item = (TypeID, &'a Type)> + 'a {
        self.world.iter()
    }
}

fn emit_preamble(out: &mut Outfile, namespace: &str) -> Result<()> {
    *out += "#pragma once";
    out.wln("#include <array>");
    out.wln("#include <cstddef>");
    out.wln("#include <cstdint>");
    out.wln("#include <limits>");
    out.wln("#include <type_traits>");
    out.wln("#include <variant>");
    out.wln("#include <vector>");
    out.wln("#include <utility>");
    out.newline();
    out.wln("// Reader concept: bool read_bytes(char*, size_t);");
    out.wln("// Writer concept: bool write_bytes(char const*, size_t);");
    write!(out, "namespace {} ", namespace)?;

    Ok(())
}

fn emit_helpers(out: &mut impl Sink) {
    out.wln("template <class Reader>");
    out.wln("inline bool read_bytes(Reader& reader, void* dst, size_t n)");
    {
        let mut idt = out.indent();
        idt.wln("return reader.read_bytes(reinterpret_cast<char*>(dst), n);");
    }
    out.newline();
    out.wln("template <class Writer>");
    out.wln("inline bool write_bytes(Writer& writer, void const* src, size_t n)");
    {
        let mut idt = out.indent();
        idt.wln("return writer.write_bytes(reinterpret_cast<char const*>(src), n);");
    }
    out.newline();
    out.wln("template <class Reader, class T>");
    out.wln("inline bool read_scalar(Reader& reader, T& value)");
    {
        let mut idt = out.indent();
        idt.wln("static_assert(std::is_trivially_copyable_v<T>);");
        idt.wln("return read_bytes(reader, &value, sizeof(T));");
    }
    out.newline();
    out.wln("template <class Writer, class T>");
    out.wln("inline bool write_scalar(Writer& writer, T const& value)");
    {
        let mut idt = out.indent();
        idt.wln("static_assert(std::is_trivially_copyable_v<T>);");
        idt.wln("return write_bytes(writer, &value, sizeof(T));");
    }
    out.newline();
    out.wln("template <class Reader, class T>");
    out.wln("inline bool read_value(Reader& reader, T& value)");
    {
        let mut idt = out.indent();
        idt.wln("if constexpr (std::is_trivially_copyable_v<T>)");
        {
            let mut inner = idt.indent();
            inner.wln("return read_scalar(reader, value);");
        }
        idt.wln("else");
        {
            let mut inner = idt.indent();
            inner.wln("return read(reader, value);");
        }
    }
    out.newline();
    out.wln("template <class Writer, class T>");
    out.wln("inline bool write_value(Writer& writer, T const& value)");
    {
        let mut idt = out.indent();
        idt.wln("if constexpr (std::is_trivially_copyable_v<T>)");
        {
            let mut inner = idt.indent();
            inner.wln("return write_scalar(writer, value);");
        }
        idt.wln("else");
        {
            let mut inner = idt.indent();
            inner.wln("return write(writer, value);");
        }
    }
}

fn emit_type_def(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id).to_string();

    match &ty.kind {
        TypeKind::Alias(alias) => {
            let target = ctx.cpp_type(alias.other)?;
            out.wln(&format!("using {} = {};", name, target));
        }
        TypeKind::Pack(pack) => {
            out.wln("#pragma pack(push, 1)");
            out.wln(&format!("struct {}", name));
            {
                let mut idt = out.indent();
                for m in &pack.members {
                    let mty = ctx.cpp_type(m.ty)?;
                    idt.wln(&format!("{} {}{{}};", mty, sanitize(&m.name)));
                }
            }
            out.wln(";");
            out.wln("#pragma pack(pop)");
            out.wln(&format!(
                "static_assert(std::is_trivially_copyable_v<{}>, \"pack must be POD\");",
                name
            ));
        }
        TypeKind::Enum(enm) => {
            let base = ctx.cpp_type(enm.underlying)?;
            out.wln(&format!("enum class {} : {} ", name, base));
            {
                let mut idt = out.indent();
                for m in &enm.members {
                    idt.wln(&format!("{} = {},", sanitize(&m.name), m.value));
                }
                if let Some(default) = &enm.default {
                    idt.wln(&format!("{} = {},", sanitize(&default.name), default.value));
                }
            }
            out.wln(";");
        }
        TypeKind::Bitfld(bitfld) => {
            out.wln(&format!("struct {} ", name));
            {
                let mut idt = out.indent();
                for m in &bitfld.members {
                    let mty = ctx.cpp_type(m.underlying)?;
                    idt.wln(&format!("{} {}{{}};", mty, sanitize(&m.name)));
                }
            }
            out.wln(";");
        }
        TypeKind::Variant(variant) => {
            let disc_ty = ctx.cpp_type(variant.discriminant)?;

            let mut payload_types = Vec::new();
            for m in &variant.members {
                payload_types.push(ctx.cpp_type(m.ty)?);
            }

            out.wln(&format!(
                "using {}_value = std::variant<{}>;",
                name,
                payload_types.join(", ")
            ));

            out.wln(&format!("struct {} ", name));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{} tag{{}};", disc_ty));
                idt.wln(&format!("{}_value value{{}};", name));
            }
            out.wln(";");
        }
        TypeKind::Sequence(seq) => {
            out.wln(&format!("struct {} ", name));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    let mty = ctx.cpp_type(m.ty)?;
                    idt.wln(&format!("{} {}{{}};", mty, sanitize(&m.name)));
                }
            }
            out.wln(";");
        }
        TypeKind::DynamicArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type)?;
            out.wln(&format!("using {} = std::vector<{}>;", name, value_ty));
        }
        TypeKind::FixedArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type)?;
            out.wln(&format!(
                "using {} = std::array<{}, {}>;",
                name, value_ty, arr.count
            ));
        }
        TypeKind::Primitive(_) | TypeKind::Void => {}
    }

    out.newline();

    Ok(())
}

fn emit_forward_decls(_ctx: &CppContext, out: &mut impl Sink, ty: &Type) {
    if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
        return;
    }
    let name = sanitize(ty.ident.to_string());
    out.wln(&format!(
        "template <class Reader> bool read(Reader&, {}&);",
        name
    ));
    out.wln(&format!(
        "template <class Writer> bool write(Writer&, {} const&);",
        name
    ));
}

fn emit_rw_impl(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id).to_string();
    match &ty.kind {
        TypeKind::Alias(_alias) => {
            // no functions needed. we are using a typedef.
        }
        TypeKind::Pack(_) => {
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("return read_scalar(reader, value);");
            }
            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("return write_scalar(writer, value);");
            }
        }
        TypeKind::Enum(enm) => {
            let base = ctx.cpp_type(enm.underlying)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{} raw{{}};", base));
                idt.wln("if (!read_scalar(reader, raw)) return false;");
                idt.wln("switch (raw)");
                {
                    let mut sw = idt.indent();
                    for m in &enm.members {
                        sw.wln(&format!(
                            "case {}: value = {}::{}; return true;",
                            m.value,
                            name,
                            sanitize(&m.name)
                        ));
                    }
                    if let Some(default) = &enm.default {
                        sw.wln(&format!(
                            "default: value = {}::{}; return true;",
                            name,
                            sanitize(&default.name)
                        ));
                    } else {
                        sw.wln("default: return false;");
                    }
                }
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("auto raw = static_cast<{}>(value);", base));
                idt.wln("return write_scalar(writer, raw);");
            }
        }
        TypeKind::Bitfld(bitfld) => {
            let under_prim = ctx.underlying_primitive(bitfld.underlying)?;
            let base = map_primitive(under_prim)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{} raw{{}};", base));
                idt.wln("if (!read_scalar(reader, raw)) return false;");
                idt.wln("auto bits = static_cast<uint64_t>(raw);");
                for m in &bitfld.members {
                    let start = m.range.start();
                    let end = m.range.end();
                    let width = end - start + 1;
                    let mask = (1u128 << width) - 1;
                    let mty = ctx.cpp_type(m.underlying)?;
                    idt.wln(&format!(
                        "{} = static_cast<{}>((bits >> {}) & 0x{:X}ull);",
                        assign_target(&m.name),
                        mty,
                        start,
                        mask
                    ));
                }
                idt.wln("return true;");
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("uint64_t raw = 0;");
                for m in &bitfld.members {
                    let start = m.range.start();
                    let end = m.range.end();
                    let width = end - start + 1;
                    let mask = (1u128 << width) - 1;
                    idt.wln(&format!(
                        "raw |= (static_cast<uint64_t>({}) & 0x{:X}ull) << {};",
                        sanitize(&m.name),
                        mask,
                        start
                    ));
                }
                idt.wln(&format!("{} out = static_cast<{}>(raw);", base, base));
                idt.wln("return write_scalar(writer, out);");
            }
        }
        TypeKind::Variant(variant) => {
            let disc_ty = ctx.cpp_type(variant.discriminant)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{} tag{{}};", disc_ty));
                idt.wln("if (!read_scalar(reader, tag)) return false;");
                idt.wln("switch (tag)");
                {
                    let mut sw = idt.indent();
                    for m in &variant.members {
                        let target_ty = ctx.cpp_type(m.ty)?;
                        sw.wln(&format!("case {}:", m.value));
                        {
                            let mut body = sw.indent();
                            body.wln(&format!("{} payload{{}};", target_ty));
                            body.wln("if (!read_value(reader, payload)) return false;");
                            body.wln(&format!("value.tag = tag;"));
                            body.wln(&format!("value.value = std::move(payload);"));
                            body.wln("return true;");
                        }
                    }
                    sw.wln("default: return false;");
                }
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("switch (value.value.index())");
                {
                    let mut sw = idt.indent();
                    for (idx, m) in variant.members.iter().enumerate() {
                        sw.wln(&format!("case {}: ", idx));
                        {
                            let mut body = sw.indent();
                            body.wln(&format!(
                                "auto const& payload = std::get<{}>(value.value);",
                                idx
                            ));
                            body.wln(&format!(
                                "auto tag = static_cast<{}>({});",
                                disc_ty, m.value
                            ));
                            body.wln("if (!write_scalar(writer, tag)) return false;");
                            body.wln("return write_value(writer, payload);");
                        }
                    }
                    sw.wln("default: return false;");
                }
            }
        }
        TypeKind::Sequence(seq) => {
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    idt.wln(&format!(
                        "if (!read_value(reader, {})) return false;",
                        assign_target(&m.name)
                    ));
                }
                idt.wln("return true;");
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    idt.wln(&format!(
                        "if (!write_value(writer, value.{})) return false;",
                        sanitize(&m.name)
                    ));
                }
                idt.wln("return true;");
            }
        }
        TypeKind::DynamicArray(arr) => {
            let size_ty = ctx.cpp_type(arr.size_type)?;
            let value_ty = ctx.cpp_type(arr.value_type)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!("{} count_raw{{}};", size_ty));
                idt.wln("if (!read_scalar(reader, count_raw)) return false;");
                idt.wln("uint64_t count = static_cast<uint64_t>(count_raw);");
                idt.wln("if (count > static_cast<uint64_t>(std::numeric_limits<size_t>::max())) return false;");
                idt.wln("value.clear();");
                idt.wln("value.reserve(static_cast<size_t>(count));");
                idt.wln("for (size_t i = 0; i < static_cast<size_t>(count); ++i)");
                {
                    let mut body = idt.indent();
                    body.wln(&format!("{} elem{{}};", value_ty));
                    body.wln("if (!read_value(reader, elem)) return false;");
                    body.wln("value.push_back(std::move(elem));");
                }
                idt.wln("return true;");
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("auto count = value.size();");
                let max_expr = match ctx.underlying_primitive(arr.size_type)? {
                    Primitive {
                        dtype: Datatype::Integer,
                        sign: Signedness::Unsigned,
                        width: BitWidth::W8,
                    } => "std::numeric_limits<uint8_t>::max()",
                    Primitive {
                        dtype: Datatype::Integer,
                        sign: Signedness::Unsigned,
                        width: BitWidth::W16,
                    } => "std::numeric_limits<uint16_t>::max()",
                    Primitive {
                        dtype: Datatype::Integer,
                        sign: Signedness::Unsigned,
                        width: BitWidth::W32,
                    } => "std::numeric_limits<uint32_t>::max()",
                    Primitive {
                        dtype: Datatype::Integer,
                        sign: Signedness::Unsigned,
                        width: BitWidth::W64,
                    } => "std::numeric_limits<uint64_t>::max()",
                    _ => "std::numeric_limits<uint64_t>::max()",
                };
                idt.wln(&format!(
                    "if (count > static_cast<size_t>({})) return false;",
                    max_expr
                ));
                idt.wln(&format!(
                    "{} count_raw = static_cast<{}>(count);",
                    size_ty, size_ty
                ));
                idt.wln("if (!write_scalar(writer, count_raw)) return false;");
                idt.wln("for (auto const& elem : value)");
                {
                    let mut body = idt.indent();
                    body.wln("if (!write_value(writer, elem)) return false;");
                }
                idt.wln("return true;");
            }
        }
        TypeKind::FixedArray(arr) => {
            let _value_ty = ctx.cpp_type(arr.value_type)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln(&format!(
                    "for (auto& elem : value) {{ if (!read_value(reader, elem)) return false; }}"
                ));
                idt.wln("return true;");
            }

            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));
            {
                let mut idt = out.indent();
                idt.wln("for (auto const& elem : value)");
                {
                    let mut body = idt.indent();
                    body.wln("if (!write_value(writer, elem)) return false;");
                }
                idt.wln("return true;");
            }
        }
        TypeKind::Primitive(_) | TypeKind::Void => {}
    }
    Ok(())
}

fn map_primitive(p: Primitive) -> anyhow::Result<String> {
    let s = match (p.dtype, p.sign, p.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => "std::uint8_t",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => "std::uint16_t",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => "std::uint32_t",
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => "std::uint64_t",
        (Datatype::Integer, Signedness::Signed, BitWidth::W8) => "std::int8_t",
        (Datatype::Integer, Signedness::Signed, BitWidth::W16) => "std::int16_t",
        (Datatype::Integer, Signedness::Signed, BitWidth::W32) => "std::int32_t",
        (Datatype::Integer, Signedness::Signed, BitWidth::W64) => "std::int64_t",
        (Datatype::Float, _, BitWidth::W32) => "float",
        (Datatype::Float, _, BitWidth::W64) => "double",
        _ => bail!("unsupported primitive type"),
    };
    Ok(s.into())
}

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

fn assign_target(name: &str) -> String {
    format!("value.{}", sanitize(name))
}
