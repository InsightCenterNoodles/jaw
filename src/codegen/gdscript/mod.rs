use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use anyhow::{Result, bail};

use crate::{
    GlobalOptions,
    codegen::{
        Outfile, Sink,
        wire_size::{fixed_wire_size, minimum_wire_size},
    },
    compile::{
        Bitfld, Const, Datatype, Enum, PrimitiveLiteral, StructMember, TypeID, TypeKind, Variant,
        World,
    },
};

const MAX_BUFFER_SIZE: u64 = i32::MAX as u64;

pub(super) fn emit(world: &World, options: &GlobalOptions, out: &mut Outfile) -> Result<()> {
    let ctx = Context::new(world, options.guard_array_size);
    for line in include_str!("runtime.gd").lines() {
        out.wln(line);
    }
    emit_scalars(&mut out.indent());

    for (id, ty) in world.iter() {
        let name = ctx.name(id);
        match &ty.kind {
            TypeKind::Bool | TypeKind::Primitive(_) | TypeKind::Void => {}
            TypeKind::Const(value) => {
                out.newline();
                out.wln(&format!(
                    "const {name}: {} = {}",
                    ctx.type_name(value.ty, false),
                    literal(value)
                ));
            }
            kind => {
                out.newline();
                out.newline();
                out.wln(&format!("class {name}:"));
                let mut body = out.indent();
                match kind {
                    TypeKind::Pack(record) => {
                        emit_record(&ctx, &mut body, name, &record.members, true)
                    }
                    TypeKind::Sequence(record) => {
                        emit_record(&ctx, &mut body, name, &record.members, false)
                    }
                    TypeKind::Enum(value) => emit_enum(&ctx, &mut body, name, value),
                    TypeKind::Bitfld(value) => emit_bitfield(&ctx, &mut body, name, value),
                    TypeKind::Variant(value) => emit_variant(&ctx, &mut body, name, value),
                    TypeKind::DynamicArray(value) => emit_array(
                        &ctx,
                        &mut body,
                        name,
                        value.value_type,
                        ArrayLength::Dynamic(value.size_type),
                    ),
                    TypeKind::FixedArray(value) => {
                        if value.count > MAX_BUFFER_SIZE {
                            bail!(
                                "fixed array {} exceeds the Godot container size limit",
                                ty.ident
                            );
                        }
                        emit_array(
                            &ctx,
                            &mut body,
                            name,
                            value.value_type,
                            ArrayLength::Fixed(value.count),
                        );
                    }
                    _ => unreachable!(),
                }
                emit_public_api(&mut body);
            }
        }
    }
    Ok(())
}

struct Context<'a> {
    world: &'a World,
    names: HashMap<TypeID, String>,
    array_guard: Option<u64>,
}

impl<'a> Context<'a> {
    fn new(world: &'a World, array_guard: Option<u64>) -> Self {
        let mut used = reserved_names();
        let names = world
            .iter()
            .map(|(id, ty)| (id, claim_name(&mut used, &format!("Jaw_{}", ty.ident))))
            .collect();
        Self {
            world,
            names,
            array_guard,
        }
    }

    fn name(&self, id: TypeID) -> &str {
        &self.names[&id]
    }

    fn underlying(&self, mut id: TypeID) -> TypeID {
        while let TypeKind::Const(value) = &self.world.lookup(id).kind {
            id = value.ty;
        }
        id
    }

    fn kind(&self, id: TypeID) -> &TypeKind {
        &self.world.lookup(self.underlying(id)).kind
    }

    fn type_name(&self, id: TypeID, packed: bool) -> &str {
        match self.kind(id) {
            TypeKind::Bool if packed => "WireBool",
            TypeKind::Bool => "bool",
            TypeKind::Primitive(value) if value.dtype == Datatype::Float => "float",
            TypeKind::Primitive(_) => "int",
            _ => self.name(self.underlying(id)),
        }
    }

    fn read_expr(&self, id: TypeID, packed: bool) -> String {
        match self.kind(id) {
            TypeKind::Bool if packed => "WireBool.new(_jaw.read_u8())".into(),
            TypeKind::Bool => "_jaw.read_bool()".into(),
            TypeKind::Primitive(value) => format!("_jaw.read_{value}()"),
            _ => format!("{}._read(_jaw, _depth + 1)", self.type_name(id, packed)),
        }
    }

    fn emit_write(&self, out: &mut impl Sink, id: TypeID, packed: bool, value: &str) {
        let scalar = match self.kind(id) {
            TypeKind::Bool if !packed => Some("bool".to_owned()),
            TypeKind::Primitive(primitive) => Some(primitive.to_string()),
            _ => None,
        };
        if let Some(suffix) = scalar {
            out.wln(&format!("_jaw.write_{suffix}({value})"));
            return;
        }

        out.wln(&format!("if {value} == null:"));
        out.indent()
            .wln("_jaw.fail(ERR_INVALID_DATA, \"Missing value\", _jaw.position())");
        out.wln("else:");
        let mut body = out.indent();
        if matches!(self.kind(id), TypeKind::Bool) {
            body.wln(&format!("_jaw.write_u8({value}.raw_value)"));
        } else {
            body.wln(&format!("{value}._write(_jaw, _depth + 1)"));
        }
    }
}

fn emit_scalars(out: &mut impl Sink) {
    let methods = [
        ("u8", "u8", 1, "int", Some((0, u8::MAX as i64))),
        ("u16", "u16", 2, "int", Some((0, u16::MAX as i64))),
        ("u32", "u32", 4, "int", Some((0, u32::MAX as i64))),
        ("u64", "u64", 8, "int", None),
        ("i8", "8", 1, "int", Some((i8::MIN as i64, i8::MAX as i64))),
        (
            "i16",
            "16",
            2,
            "int",
            Some((i16::MIN as i64, i16::MAX as i64)),
        ),
        (
            "i32",
            "32",
            4,
            "int",
            Some((i32::MIN as i64, i32::MAX as i64)),
        ),
        ("i64", "64", 8, "int", None),
        ("f32", "float", 4, "float", None),
        ("f64", "double", 8, "float", None),
    ];
    for (suffix, stream_suffix, size, ty, bounds) in methods {
        out.newline();
        out.wln(&format!("func read_{suffix}() -> {ty}:"));
        out.indent().wln(&format!(
            "return stream.get_{stream_suffix}() if can_read({size}) else {}",
            if ty == "int" { "0" } else { "0.0" }
        ));
        out.newline();
        out.wln(&format!("func write_{suffix}(value: {ty}) -> void:"));
        let mut write = out.indent();
        if let Some((min, max)) = bounds {
            write.wln(&format!("if value < {min} or value > {max}:"));
            let mut invalid = write.indent();
            invalid.wln(&format!(
                "fail(ERR_INVALID_DATA, \"Value is outside {suffix}\", position())"
            ));
            invalid.wln("return");
        }
        write.wln(&format!("if can_write({size}):"));
        write
            .indent()
            .wln(&format!("stream.put_{stream_suffix}(value)"));
    }
}

fn emit_public_api(out: &mut impl Sink) {
    out.newline();
    out.wln("static func decode(data: PackedByteArray) -> JawResult:");
    out.indent().wln("return _JawCodec.decode(data, _read)");
    out.newline();
    out.wln("func encode() -> JawResult:");
    out.indent().wln("return _JawCodec.encode(_write)");
}

fn read_body<'a>(out: &'a mut impl Sink, name: &str) -> impl Sink + 'a {
    out.newline();
    out.wln(&format!(
        "static func _read(_jaw: _JawCodec, _depth: int) -> {name}:"
    ));
    let mut body = out.indent();
    body.wln("if not _jaw.can_enter(_depth):");
    body.indent().wln("return null");
    body
}

fn write_body(out: &mut impl Sink) -> impl Sink + '_ {
    out.newline();
    out.wln("func _write(_jaw: _JawCodec, _depth: int) -> void:");
    let mut body = out.indent();
    body.wln("if not _jaw.can_enter(_depth):");
    body.indent().wln("return");
    body
}

fn emit_record(
    ctx: &Context,
    out: &mut impl Sink,
    name: &str,
    members: &[StructMember],
    packed: bool,
) {
    let mut used = reserved_names();
    let fields: Vec<_> = members
        .iter()
        .map(|field| (claim_field(&mut used, &field.name), field.ty))
        .collect();
    for (field, ty) in &fields {
        out.wln(&format!("var {field}: {}", ctx.type_name(*ty, packed)));
    }
    {
        let mut read = read_body(out, name);
        read.wln(&format!("var _value := {name}.new()"));
        for (field, ty) in &fields {
            read.wln(&format!("_value.{field} = {}", ctx.read_expr(*ty, packed)));
            read.wln("if _jaw.result.error != OK:");
            read.indent().wln("return null");
        }
        read.wln("return _value");
    }
    let mut write = write_body(out);
    for (field, ty) in &fields {
        ctx.emit_write(&mut write, *ty, packed, &format!("self.{field}"));
        write.wln("if _jaw.result.error != OK:");
        write.indent().wln("return");
    }
}

fn emit_raw_field(out: &mut impl Sink) {
    out.wln("var raw_value: int");
    out.newline();
    out.wln("func _init(raw: int = 0) -> void:");
    out.indent().wln("raw_value = raw");
}

fn emit_raw_codec(ctx: &Context, out: &mut impl Sink, name: &str, underlying: TypeID) {
    {
        let mut read = read_body(out, name);
        read.wln(&format!(
            "return {name}.new({})",
            ctx.read_expr(underlying, false)
        ));
    }
    ctx.emit_write(&mut write_body(out), underlying, false, "raw_value");
}

fn emit_enum(ctx: &Context, out: &mut impl Sink, name: &str, value: &Enum) {
    let mut used = reserved_names();
    let members: Vec<_> = value
        .members
        .iter()
        .map(|member| {
            let constant = claim_name(&mut used, &format!("VALUE_{}", member.name));
            out.wln(&format!("const {constant}: int = {}", member.value));
            (constant, &member.name)
        })
        .collect();
    out.newline();
    emit_raw_field(out);
    out.newline();
    out.wln("func member_name() -> String:");
    {
        let mut body = out.indent();
        body.wln("match raw_value:");
        let mut cases = body.indent();
        for (constant, original) in members {
            cases.wln(&format!("{constant}:"));
            cases
                .indent()
                .wln(&format!("return {}", string_literal(original)));
        }
        cases.wln("_:");
        cases.indent().wln("return \"\"");
    }
    emit_raw_codec(ctx, out, name, value.underlying);
}

fn emit_bitfield(ctx: &Context, out: &mut impl Sink, name: &str, value: &Bitfld) {
    emit_raw_field(out);
    let mut used = reserved_names();
    used.insert("raw_value".into());
    for member in &value.members {
        let field = claim_field(&mut used, &member.name);
        let ty = ctx.type_name(member.underlying, false);
        let enum_member = matches!(ctx.kind(member.underlying), TypeKind::Enum(_));
        let start = member.range.start();
        let width = member.range.end() - start + 1;
        let field_mask = ((1u128 << width) - 1) as u64;
        let mask = int_literal(field_mask as i64);
        let shifted_mask = int_literal((field_mask << start) as i64);
        let extract = format!("(raw_value >> {start}) & {mask}");
        out.newline();
        out.wln(&format!("var {field}: {ty}:"));
        let mut property = out.indent();
        property.wln("get:");
        {
            let mut get = property.indent();
            if enum_member {
                get.wln(&format!("return {ty}.new({extract})"));
            } else {
                get.wln(&format!("return {extract}"));
            }
        }
        property.wln("set(_value):");
        let mut set = property.indent();
        let source = if enum_member {
            set.wln("if _value == null:");
            set.indent().wln("return");
            "_value.raw_value"
        } else {
            "_value"
        };
        let bits = format!("({source} & {mask})");
        let shifted_bits = if *start == 0 {
            bits
        } else {
            format!("({bits} << {start})")
        };
        set.wln(&format!(
            "raw_value = (raw_value & ~{shifted_mask}) | {shifted_bits}"
        ));
    }

    let underlying = match ctx.kind(value.underlying) {
        TypeKind::Enum(value) => value.underlying,
        _ => value.underlying,
    };
    emit_raw_codec(ctx, out, name, underlying);
}

fn emit_variant(ctx: &Context, out: &mut impl Sink, name: &str, value: &Variant) {
    let mut used = reserved_names();
    let cases: Vec<_> = value
        .members
        .iter()
        .map(|member| {
            let tag = claim_name(&mut used, &format!("TAG_{}", member.name));
            let factory = claim_name(&mut used, &format!("make_{}", member.name));
            out.wln(&format!(
                "const {tag}: int = {}",
                int_literal(member.value as i64)
            ));
            (tag, factory, member.ty)
        })
        .collect();
    out.newline();
    out.wln("var tag: int");
    out.wln("var payload: Variant");
    out.newline();
    out.wln("func _init(initial_tag: int = 0, initial_payload: Variant = null) -> void:");
    {
        let mut body = out.indent();
        body.wln("tag = initial_tag");
        body.wln("payload = initial_payload");
    }
    for (tag, factory, ty) in &cases {
        out.newline();
        if matches!(ctx.kind(*ty), TypeKind::Void) {
            out.wln(&format!("static func {factory}() -> {name}:"));
            out.indent().wln(&format!("return {name}.new({tag})"));
        } else {
            out.wln(&format!(
                "static func {factory}(value: {}) -> {name}:",
                ctx.type_name(*ty, false)
            ));
            out.indent()
                .wln(&format!("return {name}.new({tag}, value)"));
        }
    }
    {
        let mut read = read_body(out, name);
        read.wln("var _offset := _jaw.position()");
        read.wln(&format!(
            "var _tag := {}",
            ctx.read_expr(value.discriminant, false)
        ));
        read.wln("if _jaw.result.error != OK:");
        read.indent().wln("return null");
        read.wln("match _tag:");
        let mut branches = read.indent();
        for (tag, _, ty) in &cases {
            branches.wln(&format!("{tag}:"));
            let mut arm = branches.indent();
            if matches!(ctx.kind(*ty), TypeKind::Void) {
                arm.wln(&format!("return {name}.new(_tag)"));
            } else {
                arm.wln(&format!(
                    "return {name}.new(_tag, {})",
                    ctx.read_expr(*ty, false)
                ));
            }
        }
        branches.wln("_:");
        let mut invalid = branches.indent();
        invalid.wln("_jaw.fail(ERR_INVALID_DATA, \"Unknown variant tag\", _offset)");
        invalid.wln("return null");
    }
    let mut write = write_body(out);
    write.wln("match tag:");
    let mut branches = write.indent();
    for (tag, _, ty) in cases {
        branches.wln(&format!("{tag}:"));
        let mut arm = branches.indent();
        let is_void = matches!(ctx.kind(ty), TypeKind::Void);
        let wrong_type = if is_void {
            "payload != null".into()
        } else {
            format!("not (payload is {})", ctx.type_name(ty, false))
        };
        arm.wln(&format!("if {wrong_type}:"));
        {
            let mut invalid = arm.indent();
            invalid.wln("_jaw.fail(ERR_INVALID_DATA, \"Variant payload does not match the tag\", _jaw.position())");
            invalid.wln("return");
        }
        ctx.emit_write(&mut arm, value.discriminant, false, "tag");
        if !is_void {
            arm.wln(&format!(
                "var _payload: {} = payload",
                ctx.type_name(ty, false)
            ));
            ctx.emit_write(&mut arm, ty, false, "_payload");
        }
    }
    branches.wln("_:");
    branches
        .indent()
        .wln("_jaw.fail(ERR_INVALID_DATA, \"Unknown variant tag\", _jaw.position())");
}

#[derive(Clone, Copy)]
enum ArrayLength {
    Fixed(u64),
    Dynamic(TypeID),
}

fn emit_array(
    ctx: &Context,
    out: &mut impl Sink,
    name: &str,
    element: TypeID,
    length: ArrayLength,
) {
    let bytes = matches!(ctx.kind(element), TypeKind::Primitive(primitive) if primitive.is_u8());
    let packed = matches!(length, ArrayLength::Fixed(_));
    let element_name = ctx.type_name(element, packed);
    let storage = if bytes {
        "PackedByteArray".into()
    } else {
        format!("Array[{element_name}]")
    };
    let empty = if bytes { "PackedByteArray()" } else { "[]" };
    out.wln(&format!("var values: {storage}"));
    out.newline();
    out.wln(&format!("func _init(items: {storage} = {empty}) -> void:"));
    out.indent().wln("values = items");

    let min_size = minimum_wire_size(ctx.world, element)
        .unwrap_or(0)
        .min(MAX_BUFFER_SIZE);
    let mut max_count = MAX_BUFFER_SIZE;
    if let Some(guard) = ctx.array_guard {
        let element_size = fixed_wire_size(ctx.world, element).unwrap_or(1).max(1);
        max_count = max_count.min(guard / element_size);
    }
    {
        let mut read = read_body(out, name);
        read.wln("var _offset := _jaw.position()");
        let count = match length {
            ArrayLength::Fixed(count) => count.to_string(),
            ArrayLength::Dynamic(ty) => ctx.read_expr(ty, false),
        };
        read.wln(&format!("var _count := {count}"));
        read.wln(&format!(
            "if not _jaw.check_array(_count, {max_count}, {min_size}, _offset):"
        ));
        read.indent().wln("return null");
        if bytes {
            read.wln(&format!("return {name}.new(_jaw.read_bytes(_count))"));
        } else {
            read.wln(&format!("var _items: {storage} = []"));
            read.wln("for _index in range(_count):");
            {
                let mut loop_body = read.indent();
                loop_body.wln(&format!(
                    "var _item: {element_name} = {}",
                    ctx.read_expr(element, packed)
                ));
                loop_body.wln("if _jaw.result.error != OK:");
                loop_body.indent().wln("return null");
                loop_body.wln("_items.append(_item)");
            }
            read.wln(&format!("return {name}.new(_items)"));
        }
    }
    let mut write = write_body(out);
    if min_size == 0 {
        write.wln("if not _jaw.check_empty_items(values.size(), _jaw.position()):");
        write.indent().wln("return");
    }
    match length {
        ArrayLength::Fixed(count) => {
            write.wln(&format!("if values.size() != {count}:"));
            let mut invalid = write.indent();
            invalid.wln(
                "_jaw.fail(ERR_INVALID_DATA, \"Incorrect fixed array length\", _jaw.position())",
            );
            invalid.wln("return");
        }
        ArrayLength::Dynamic(ty) => {
            ctx.emit_write(&mut write, ty, false, "values.size()");
            write.wln("if _jaw.result.error != OK:");
            write.indent().wln("return");
        }
    }
    if bytes {
        write.wln("_jaw.write_bytes(values)");
    } else {
        write.wln("for _item in values:");
        let mut loop_body = write.indent();
        ctx.emit_write(&mut loop_body, element, packed, "_item");
        loop_body.wln("if _jaw.result.error != OK:");
        loop_body.indent().wln("return");
    }
}

fn literal(value: &Const) -> String {
    match value.value {
        PrimitiveLiteral::Boolean(value) => value.to_string(),
        PrimitiveLiteral::Integer(value) => int_literal(value as i64),
        PrimitiveLiteral::Real(value) => format!("{value:?}"),
    }
}

fn int_literal(value: i64) -> String {
    if value == i64::MIN {
        "(-9223372036854775807 - 1)".into()
    } else {
        value.to_string()
    }
}

fn string_literal(value: &str) -> String {
    let mut literal = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' | '\\' => {
                literal.push('\\');
                literal.push(ch);
            }
            ch if ch.is_control() => write!(literal, "\\u{:04x}", ch as u32).unwrap(),
            ch => literal.push(ch),
        }
    }
    literal.push('"');
    literal
}

fn claim_name(used: &mut HashSet<String>, original: &str) -> String {
    let mut name: String = original
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit() || c == '_') {
        name.insert_str(0, "jaw_");
    }
    while !used.insert(name.clone()) {
        name.push('_');
    }
    name
}

fn claim_field(used: &mut HashSet<String>, original: &str) -> String {
    // Godot rejects members that shadow native classes. Their names begin with uppercase letters.
    if original.starts_with(|c: char| c.is_ascii_uppercase()) {
        claim_name(used, &format!("jaw_{original}"))
    } else {
        claim_name(used, original)
    }
}

fn reserved_names() -> HashSet<String> {
    // Language tokens, names referenced by the runtime, and generated/base class members.
    "
    and as assert await break breakpoint class class_name const continue elif else enum
    extends false for func if in is match namespace not null or pass preload return self
    signal static super trait true var void when while yield
    bool int float String StringName NodePath RID Object Variant Callable Signal Array Dictionary
    Vector2 Vector2i Vector3 Vector3i Vector4 Vector4i Rect2 Rect2i Transform2D Transform3D
    Plane Quaternion AABB Basis Projection Color PackedByteArray PackedInt32Array PackedInt64Array
    PackedFloat32Array PackedFloat64Array PackedStringArray PackedVector2Array PackedVector3Array
    PackedVector4Array PackedColorArray RefCounted StreamPeerBuffer
    OK ERR_INVALID_DATA ERR_FILE_EOF Error INF NAN PI TAU range
    JawResult WireBool decode encode
    new free get set call callv call_deferred get_class is_class has_method has_signal
    get_script set_script get_instance_id emit_signal connect disconnect
    reference unreference init_ref get_reference_count
    "
    .split_whitespace()
    .map(str::to_owned)
    .collect()
}
