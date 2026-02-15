use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{BitWidth, Datatype, DynamicArray, Signedness, Type, TypeID, TypeKind},
};

use anyhow::{Result, bail};

pub fn emit_dynarr(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    arr: &DynamicArray,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);

    let view_type = ctx.rust_view_type(id);

    {
        let elem = ctx.name_of(arr.value_type);
        out.wln("#[derive(Debug, Clone, PartialEq)]");
        out.wln(&format!("pub struct {name}(pub Vec<{elem}>);"));
        out.newline();
    }

    {
        let elem_view = ctx.rust_write_decl_type(arr.value_type, "'a")?;
        out.wln("#[derive(Debug, PartialEq)]");
        out.wln(&format!(
            "pub struct {view_type}<'a>(pub &'a [{elem_view}]);"
        ));
        out.newline();
    }

    out.newline();

    //
    reader(ctx, out, id, arr)?;

    writer(ctx, out, id, arr)?;

    Ok(())
}

fn reader(ctx: &RustContext, out: &mut impl Sink, id: TypeID, arr: &DynamicArray) -> Result<()> {
    let name = ctx.name_of(id);

    let elem_name = ctx.name_of(arr.value_type);

    emit_jread(&name, None, out, |idt| {
        idt.wln(&format!(
            "let count_raw = {};",
            read_expr(ctx, arr.size_type)
        ));
        idt.wln("let count: usize = count_raw.try_into().map_err(|_| invalid_data(\"array length too large\"))?;");

        ctx.insert_optional_size_check(idt, "count", &elem_name);

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
                    let expr = read_expr(ctx, arr.value_type);
                    body.wln(&format!("out.push({expr}?);"));
                }
            }
        }
        idt.wln("Ok(Self(out))");
        Ok(())
    })
}

// Shared bound for dynamic array length encoding, based on the declared counter type.
/// Returns the maximum encodable array length for a given size-counter type.
fn max_len_for_size(ctx: &RustContext, id: TypeID) -> Result<u128> {
    let prim = ctx
        .underlying_primitive(id)
        .ok_or_else(|| anyhow::anyhow!("expected primitive-compatible type"))?;
    match (prim.dtype, prim.sign, prim.width) {
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => Ok(u8::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => Ok(u16::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => Ok(u32::MAX as u128),
        (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => Ok(u64::MAX as u128),
        _ => bail!("array size type must be an unsigned integer"),
    }
}

fn writer(ctx: &RustContext, out: &mut impl Sink, id: TypeID, arr: &DynamicArray) -> Result<()> {
    let plain_type = ctx.name_of(id);
    let view_type = ctx.rust_view_type(id);

    let lt = ctx.view_needs_lifetime(id).then(|| "'a");

    let max_len = max_len_for_size(ctx, arr.size_type)?;

    emit_jwrite(&plain_type, None, out, |idt| {
        idt.wln(&format!("{view_type}(&self.0).jaw_write(writer)"));
        Ok(())
    })?;

    emit_jwrite(&view_type, lt, out, |idt| {
        //let write_tag = write_primitive_method(ctx, variant.discriminant)?;
        idt.wln("let len = self.0.len();");
        idt.wln(&format!(
            "let max_len: usize = {}usize;",
            max_len.min(usize::MAX as u128)
        ));
        idt.wln("// Fail instead of truncating if the vector does not fit in the count type.");
        idt.wln(
            "if len > max_len { return Err(invalid_data(\"array length too large to encode\")); }",
        );
        idt.wln(&format!(
            "let count: {} = len.try_into().map_err(|_| invalid_data(\"array length too large to encode\"))?;",
            ctx.name_of(arr.size_type)
        ));
        idt.wln(&format!("{};", write_expr("count")));
        if ctx.direct_primitive(arr.value_type).is_some() {
            idt.wln("writer.write_all(bytemuck::cast_slice(self.0))?;");
        } else {
            idt.wln("for v in self.0");
            {
                let mut body = idt.indent();
                body.wln(&write_expr("v"));
            }
        }
        idt.wln("Ok(())");
        Ok(())
    })
}
