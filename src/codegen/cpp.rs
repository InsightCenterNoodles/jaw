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

        ns.wln("template <class T> struct ArrayRef;");

        emit_namespace_fwd(&ctx, &mut ns, Namespace::Read)?;
        ns.newline();
        emit_namespace_fwd(&ctx, &mut ns, Namespace::Write)?;

        ns.newline();
        emit_helpers(&mut ns);

        ns.newline();
        emit_namespace(&ctx, &mut ns, Namespace::Read)?;
        ns.newline();
        emit_namespace(&ctx, &mut ns, Namespace::Write)?;
    }

    Ok(())
}

struct CppContext<'a> {
    world: &'a World,
    names: HashMap<TypeID, String>,
    namespace: String,
}

#[derive(Clone, Copy)]
enum Namespace {
    Read,
    Write,
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

    fn name_of(&self, id: TypeID, ns: Namespace) -> String {
        format!(
            "{}{}",
            self.names
                .get(&id)
                .expect("missing generated name for type id"),
            match ns {
                Namespace::Read => "Reader",
                Namespace::Write => "Writer",
            }
        )
    }

    fn cpp_type(&self, id: TypeID, ns: Namespace) -> anyhow::Result<String> {
        let ty = self.world.lookup(id);
        match &ty.kind {
            TypeKind::Primitive(p) => Ok(map_primitive(*p)?),
            TypeKind::Void => Ok("std::monostate".into()),
            _ => Ok(self.name_of(id, ns).to_string()),
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

    fn namespace_name(&self, ns: Namespace) -> &'static str {
        match ns {
            Namespace::Read => "readers",
            Namespace::Write => "writers",
        }
    }

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

    fn is_pod(&self, id: TypeID) -> bool {
        let resolved = self.resolve_alias(id);
        let ty = self.world.lookup(resolved);

        // println!("IS POD {ty:?}");

        match &ty.kind {
            TypeKind::Alias(alias) => self.is_pod(alias.other),
            TypeKind::Pack(_) => true,
            TypeKind::Enum(_) => true,
            TypeKind::Bitfld(_) => true,
            TypeKind::Variant(_) => false,
            TypeKind::Sequence(_) => false,
            TypeKind::DynamicArray(_) => false,
            TypeKind::FixedArray(fixed_array) => self.is_pod(fixed_array.value_type),
            TypeKind::Primitive(_) => true,
            TypeKind::Void => true,
        }
    }

    fn can_bulk_array(&self, id: TypeID) -> bool {
        let ty = self.world.lookup(id);

        //println!("CAN OPTIM {ty:?}");

        match &ty.kind {
            TypeKind::DynamicArray(dynamic_array) => self.is_pod(dynamic_array.value_type),
            TypeKind::FixedArray(fixed_array) => self.is_pod(fixed_array.value_type),
            _ => false,
        }
    }

    fn is_void(&self, id: TypeID) -> bool {
        matches!(&self.world.lookup(id).kind, TypeKind::Void)
    }
}

fn emit_preamble(out: &mut Outfile, namespace: &str) -> Result<()> {
    *out += "#pragma once";
    out.wln("#include <array>");
    out.wln("#include <cstddef>");
    out.wln("#include <cstdint>");
    out.wln("#include <limits>");
    out.wln("#include <span>");
    out.wln("#include <type_traits>");
    out.wln("#include <variant>");
    out.wln("#include <utility>");
    out.newline();
    out.wln("// Reader concept: bool read_bytes(std::byte*, size_t);");
    out.wln("// Reader concept: std::span<std::byte> advance_bytes(size_t);");
    out.wln("// Writer concept: bool write_bytes(std::byte const*, size_t);");
    write!(out, "namespace {} ", namespace)?;

    Ok(())
}

fn emit_helpers(out: &mut impl Sink) {
    out.wln("template <class Reader>");
    out.wln("inline bool read_bytes(Reader& reader, void* dst, size_t n)");
    {
        let mut idt = out.indent();
        idt.wln("return reader.read_bytes(reinterpret_cast<std::byte*>(dst), n);");
    }
    out.newline();
    out.wln("template <class Writer>");
    out.wln("inline bool write_bytes(Writer& writer, void const* src, size_t n)");
    {
        let mut idt = out.indent();
        idt.wln("return writer.write_bytes(reinterpret_cast<std::byte const*>(src), n);");
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
        idt.wln("if constexpr (std::is_trivially_copyable_v<T> && std::is_arithmetic_v<T>)");
        {
            let mut inner = idt.indent();
            inner.wln("return read_scalar(reader, value);");
        }
        idt.wln("else");
        {
            let mut inner = idt.indent();
            inner.wln("return readers::read(reader, value);");
        }
    }
    out.newline();
    out.wln("template <class Writer, class T>");
    out.wln("inline bool write_value(Writer& writer, T const& value)");
    {
        let mut idt = out.indent();
        idt.wln("if constexpr (std::is_trivially_copyable_v<T> && std::is_arithmetic_v<T>)");
        {
            let mut inner = idt.indent();
            inner.wln("return write_scalar(writer, value);");
        }
        idt.wln("else");
        {
            let mut inner = idt.indent();
            inner.wln("return writers::write(writer, value);");
        }
    }

    out.newline();
    out.wln("template <class T>");
    out.wln("struct ArrayRef");
    {
        let mut idt = out.indent();
        idt.wln("const std::byte* content_ptr;");
        idt.wln("size_t t_count;");

        idt.wln("void copy_to(std::span<T> dest)");
        {
            let mut idt = idt.indent();
            idt.wln("auto count = std::min(dest.size(), t_count);");
            idt.wln("std::memcpy(dest.data(), content_ptr, sizeof(T)*count);");
        }
        idt.wln("void copy_to_vector(std::vector<T>& dest)");
        {
            let mut idt = idt.indent();
            idt.wln("dest.resize(t_count);");
            idt.wln("std::memcpy(dest.data(), content_ptr, sizeof(T)*t_count);");
        }
        idt.wln("template <class Function> void for_each(Function&& f)");
        {
            let mut idt = idt.indent();
            idt.wln("T temp;");
            idt.wln("for (size_t i = 0; i < t_count; i++)");

            {
                let mut idt = idt.indent();

                idt.wln("std::memcpy(&temp, content_ptr + (i * sizeof(T)), sizeof(T));");

                idt.wln("f(i, temp);");
            }
        }
    }
    out.wln(";");
}

fn emit_namespace_fwd(ctx: &CppContext, out: &mut impl Sink, ns: Namespace) -> anyhow::Result<()> {
    out.wln(&format!("namespace {} ", ctx.namespace_name(ns)));
    {
        let mut block = out.indent();

        for (id, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            emit_type_def_fwd(ctx, &mut block, id, ty, ns)?;
        }

        for (_, ty) in ctx.types() {
            if matches!(
                ty.kind,
                TypeKind::Primitive(_) | TypeKind::Void | TypeKind::Alias(_)
            ) {
                continue;
            }
            emit_forward_decls(&mut block, ty, ns);
        }
    }

    Ok(())
}

fn emit_namespace(ctx: &CppContext, out: &mut impl Sink, ns: Namespace) -> anyhow::Result<()> {
    out.wln(&format!("namespace {} ", ctx.namespace_name(ns)));
    {
        let mut block = out.indent();

        for (id, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            emit_type_def(ctx, &mut block, id, ty, ns)?;
        }

        block.newline();

        for (_, ty) in ctx.types() {
            if matches!(
                ty.kind,
                TypeKind::Primitive(_) | TypeKind::Void | TypeKind::Alias(_)
            ) {
                continue;
            }
            emit_forward_decls(&mut block, ty, ns);
        }

        block.newline();

        for (id, ty) in ctx.types() {
            if matches!(ty.kind, TypeKind::Primitive(_) | TypeKind::Void) {
                continue;
            }
            match ns {
                Namespace::Read => emit_read_impl(ctx, &mut block, id, ty)?,
                Namespace::Write => emit_write_impl(ctx, &mut block, id, ty)?,
            }
        }
    }

    Ok(())
}

fn emit_type_def_fwd(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
    ns: Namespace,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id, ns).to_string();

    match &ty.kind {
        TypeKind::Alias(alias) => {
            let target = ctx.cpp_type(alias.other, ns)?;
            out.wln(&format!("using {} = {};", name, target));
        }
        TypeKind::Pack(_) => {
            out.wln(&format!("struct {};", name));
        }
        TypeKind::Enum(enm) => {
            let base = ctx.cpp_type(enm.underlying, ns)?;
            out.wln(&format!("enum class {} : {};", name, base));
        }
        TypeKind::Bitfld(_) => {
            out.wln(&format!("struct {};", name));
        }
        TypeKind::Variant(_) => {
            out.wln(&format!("struct {};", name));
        }
        TypeKind::Sequence(_) => {
            out.wln(&format!("struct {};", name));
        }
        TypeKind::DynamicArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type, ns)?;
            match ns {
                Namespace::Read => {
                    if ctx.can_bulk_array(id) {
                        out.wln(&format!("using {} = ArrayRef<{}>;", name, value_ty));
                    } else {
                        out.wln(&format!("using {} = std::vector<{}>;", name, value_ty));
                    }
                }
                Namespace::Write => {
                    out.wln(&format!("using {} = std::span<{}>;", name, value_ty));
                }
            }
        }
        TypeKind::FixedArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type, ns)?;

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

fn emit_type_def(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
    ns: Namespace,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id, ns).to_string();

    match &ty.kind {
        TypeKind::Alias(alias) => {
            let target = ctx.cpp_type(alias.other, ns)?;
            out.wln(&format!("using {} = {};", name, target));
        }
        TypeKind::Pack(pack) => {
            out.wln("#pragma pack(push, 1)");
            out.wln(&format!("struct {}", name));
            {
                let mut idt = out.indent();
                for m in &pack.members {
                    let mty = ctx.cpp_type(m.ty, ns)?;
                    idt.wln(&format!("{} {};", mty, sanitize(&m.name)));
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
            let base = ctx.cpp_type(enm.underlying, ns)?;
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
                    let mty = ctx.cpp_type(m.underlying, ns)?;
                    idt.wln(&format!("{} {}{{}};", mty, sanitize(&m.name)));
                }
            }
            out.wln(";");
        }
        TypeKind::Variant(variant) => {
            let mut payload_types = Vec::new();
            for m in &variant.members {
                let target = ctx.cpp_type(m.ty, ns)?;
                if ctx.is_void(m.ty) {
                    payload_types.push("std::monostate".to_string());
                } else if matches!(ns, Namespace::Write) {
                    payload_types.push(format!("{} const*", target));
                } else {
                    payload_types.push(target);
                }
            }

            out.wln(&format!(
                "struct {} : std::variant<{}>",
                name,
                payload_types.join(", ")
            ));
            {
                let mut idt = out.indent();

                idt.wln("using variant::variant;");
                idt.wln("using variant::operator=;");
            }
            out.wln(";");
        }
        TypeKind::Sequence(seq) => {
            out.wln(&format!("struct {} ", name));
            {
                let mut idt = out.indent();
                for m in &seq.members {
                    let mty = match ns {
                        Namespace::Read => "",
                        Namespace::Write => "const&",
                    };

                    idt.wln(&format!(
                        "{} {} {};",
                        ctx.cpp_type(m.ty, ns)?,
                        mty,
                        sanitize(&m.name)
                    ));
                }
            }
            out.wln(";");
        }
        TypeKind::DynamicArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type, ns)?;
            match ns {
                Namespace::Read => {
                    if ctx.can_bulk_array(id) {
                        out.wln(&format!("using {} = ArrayRef<{}>;", name, value_ty));
                    } else {
                        out.wln(&format!("using {} = std::vector<{}>;", name, value_ty));
                    }
                }
                Namespace::Write => {
                    out.wln(&format!("using {} = std::span<{}>;", name, value_ty));
                }
            }
        }
        TypeKind::FixedArray(arr) => {
            let value_ty = ctx.cpp_type(arr.value_type, ns)?;

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

fn emit_forward_decls(out: &mut impl Sink, ty: &Type, ns: Namespace) {
    if matches!(
        ty.kind,
        TypeKind::Primitive(_) | TypeKind::Void | TypeKind::Alias(_)
    ) {
        return;
    }

    let name = sanitize(ty.ident.to_string());
    if matches!(ns, Namespace::Read) {
        out.wln(&format!(
            "template <class Reader> bool read(Reader&, {}Reader&);",
            name
        ));
    }
    if matches!(ns, Namespace::Write) {
        out.wln(&format!(
            "template <class Writer> bool write(Writer&, {}Writer const&);",
            name
        ));
    }
}

fn emit_read_impl(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id, Namespace::Read).to_string();
    match &ty.kind {
        TypeKind::Alias(_alias) => {
            // aliases use the underlying implementation
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
        }
        TypeKind::Enum(enm) => {
            let base = ctx.cpp_type(enm.underlying, Namespace::Read)?;
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
                    let mty = ctx.cpp_type(m.underlying, Namespace::Read)?;
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
        }
        TypeKind::Variant(variant) => {
            let disc_ty = ctx.cpp_type(variant.discriminant, Namespace::Read)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));

            // need a remap of index to index
            let dsl_index_to_variant_index: HashMap<_, _> = variant
                .members
                .iter()
                .enumerate()
                .map(|x| (x.1.value, x.0))
                .collect();

            {
                let mut idt = out.indent();
                idt.wln(&format!("{} tag{{}};", disc_ty));
                idt.wln("if (!read_scalar(reader, tag)) return false;");
                idt.wln("switch (tag)");
                {
                    let mut sw = idt.indent();
                    for m in &variant.members {
                        let variant_index = dsl_index_to_variant_index.get(&m.value).unwrap();
                        let target_ty = ctx.cpp_type(m.ty, Namespace::Read)?;
                        sw.wln(&format!("case {}:", m.value));
                        {
                            let mut body = sw.indent();
                            if ctx.is_void(m.ty) {
                                body.wln("value = std::monostate{};");
                                body.wln("return true;");
                            } else {
                                body.wln(&format!("{} payload{{}};", target_ty));
                                body.wln("if (!read_value(reader, payload)) return false;");
                                body.wln(&format!("value.emplace<{variant_index}>(payload);"));
                                body.wln("return true;");
                            }
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
        }
        TypeKind::DynamicArray(arr) => {
            let size_ty = ctx.cpp_type(arr.size_type, Namespace::Read)?;
            let value_ty = ctx.cpp_type(arr.value_type, Namespace::Read)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));

            if ctx.can_bulk_array(id) {
                let mut idt = out.indent();
                idt.wln(&format!("{} count_raw{{}};", size_ty));
                idt.wln("if (!read_scalar(reader, count_raw)) return false;");
                idt.wln("auto count = static_cast<uint64_t>(count_raw);");
                idt.wln("if (count > static_cast<uint64_t>(std::numeric_limits<size_t>::max())) return false;");
                idt.wln(&format!(
                    "auto byte_count = sizeof({}) * static_cast<size_t>(count);",
                    value_ty
                ));
                idt.wln("auto ptr = reader.advance_bytes(byte_count);");
                idt.wln("if (ptr.empty() && count > 0) return false;");
                idt.wln("value.content_ptr = ptr.data(); value.t_count = count;");
                idt.wln("return true;");
            } else {
                let mut idt = out.indent();
                idt.wln(&format!("{} count_raw{{}};", size_ty));
                idt.wln("if (!read_scalar(reader, count_raw)) return false;");
                idt.wln("auto count = static_cast<uint64_t>(count_raw);");
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
        }
        TypeKind::FixedArray(_) => {
            //let value_ty = ctx.cpp_type(arr.value_type, Namespace::Read)?;
            out.wln(&format!(
                "template <class Reader> inline bool read(Reader& reader, {}& value)",
                name
            ));
            if ctx.can_bulk_array(id) {
                let mut idt = out.indent();

                idt.wln("return read_scalar(reader, value);");
            } else {
                let mut idt = out.indent();
                idt.wln(&format!(
                    "for (auto& elem : value) {{ if (!read_value(reader, elem)) return false; }}"
                ));
                idt.wln("return true;");
            }
        }
        TypeKind::Primitive(_) | TypeKind::Void => {}
    }
    Ok(())
}

fn emit_write_impl(
    ctx: &CppContext,
    out: &mut impl Sink,
    id: TypeID,
    ty: &Type,
) -> anyhow::Result<()> {
    let name = ctx.name_of(id, Namespace::Write).to_string();
    match &ty.kind {
        TypeKind::Alias(_alias) => {
            // aliases use the underlying implementation
        }
        TypeKind::Pack(_) => {
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
            let base = ctx.cpp_type(enm.underlying, Namespace::Write)?;
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
                        assign_target(&m.name),
                        mask,
                        start
                    ));
                }
                idt.wln(&format!("{} out = static_cast<{}>(raw);", base, base));
                idt.wln("return write_scalar(writer, out);");
            }
        }
        TypeKind::Variant(variant) => {
            let disc_ty = ctx.cpp_type(variant.discriminant, Namespace::Write)?;
            out.wln(&format!(
                "template <class Writer> inline bool write(Writer& writer, {} const& value)",
                name
            ));

            // need a remap of index to index
            let variant_index_to_dsl_index: HashMap<_, _> = variant
                .members
                .iter()
                .enumerate()
                .map(|x| (x.0, x.1.value))
                .collect();

            {
                let mut idt = out.indent();
                idt.wln("switch (value.index())");
                {
                    let mut sw = idt.indent();
                    for (idx, m) in variant.members.iter().enumerate() {
                        sw.wln(&format!("case {}: ", idx));
                        {
                            let mut body = sw.indent();
                            if ctx.is_void(m.ty) {
                                body.wln(&format!(
                                    "auto tag = static_cast<{}>({});",
                                    disc_ty,
                                    variant_index_to_dsl_index.get(&idx).unwrap()
                                ));
                                body.wln("if (!write_scalar(writer, tag)) return false;");
                                body.wln("return true;");
                            } else {
                                body.wln(&format!("auto payload = std::get<{}>(value);", idx));
                                body.wln("if (payload == nullptr) return false;");
                                body.wln(&format!(
                                    "auto tag = static_cast<{}>({});",
                                    disc_ty,
                                    variant_index_to_dsl_index.get(&idx).unwrap()
                                ));
                                body.wln("if (!write_scalar(writer, tag)) return false;");
                                body.wln("return write_value(writer, *payload);");
                            }
                        }
                    }
                    sw.wln("default: return false;");
                }
            }
        }
        TypeKind::Sequence(seq) => {
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
            let size_ty = ctx.cpp_type(arr.size_type, Namespace::Write)?;
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
            let _value_ty = ctx.cpp_type(arr.value_type, Namespace::Write)?;
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
