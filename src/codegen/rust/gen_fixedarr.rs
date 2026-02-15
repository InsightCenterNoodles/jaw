use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{FixedArray, Type, TypeID, TypeKind},
};

use anyhow::Result;

pub fn emit_fixedarr(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    arr: &FixedArray,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);

    let view_type = ctx.rust_view_type(id);

    {
        let maybe_copy = if ctx.can_bulk_array(id).is_some() {
            "Copy,"
        } else {
            ""
        };
        let elem = ctx.name_of(arr.value_type);
        out.wln(&format!("#[derive(Debug, Clone, {}PartialEq)]", maybe_copy));
        out.wln(&format!("pub struct {name}(pub [{elem}; {}]);", arr.count));
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

fn reader(ctx: &RustContext, out: &mut impl Sink, id: TypeID, arr: &FixedArray) -> Result<()> {
    let name = ctx.name_of(id);

    let elem_name = ctx.name_of(arr.value_type);

    emit_jread(&name, None, out, |idt| {
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
                    let expr = read_expr(ctx, arr.value_type);
                    body.wln(&format!("*x = {expr}?;"));
                }
            }
        }

        idt.wln("Ok(Self(out))");
        Ok(())
    })
}

// // Shared bound for dynamic array length encoding, based on the declared counter type.
// /// Returns the maximum encodable array length for a given size-counter type.
// fn max_len_for_size(ctx: &RustContext, id: TypeID) -> Result<u128> {
//     let prim = ctx
//         .underlying_primitive(id)
//         .ok_or_else(|| anyhow::anyhow!("expected primitive-compatible type"))?;
//     match (prim.dtype, prim.sign, prim.width) {
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W8) => Ok(u8::MAX as u128),
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W16) => Ok(u16::MAX as u128),
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W32) => Ok(u32::MAX as u128),
//         (Datatype::Integer, Signedness::Unsigned, BitWidth::W64) => Ok(u64::MAX as u128),
//         _ => bail!("array size type must be an unsigned integer"),
//     }
// }

fn writer(ctx: &RustContext, out: &mut impl Sink, id: TypeID, arr: &FixedArray) -> Result<()> {
    let plain_type = ctx.name_of(id);
    let view_type = ctx.rust_view_type(id);

    let lt = ctx.view_needs_lifetime(id).then(|| "'a");

    emit_jwrite(&plain_type, None, out, |idt| {
        idt.wln(&format!("{view_type}(&self.0).jaw_write(writer)"));
        Ok(())
    })?;

    emit_jwrite(&view_type, lt, out, |idt| {
        idt.wln(&format!(
                    "if self.0.len() != {} {{ return Err(invalid_data(\"unexpected fixed array length\")); }}",
                    arr.count
                ));
        if ctx.direct_primitive(arr.value_type).is_some() {
            idt.wln("writer.write_all(bytemuck::cast_slice(self.0))?;");
        } else {
            idt.wln("for v in self.0");
            {
                let mut body = idt.indent();
                body.wln(&format!("{};", write_expr("v")));
            }
        }
        idt.wln("Ok(())");
        Ok(())
    })
}
