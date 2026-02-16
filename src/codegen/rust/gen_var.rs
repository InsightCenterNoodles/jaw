use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Type, TypeID, TypeKind, Variant, VariantMember},
};

use anyhow::Result;

pub fn emit_var(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    variant: &Variant,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);

    let view_type = ctx.rust_view_type(id);

    {
        out.wln("#[allow(non_camel_case_types)]");
        out.wln("#[derive(Debug, Clone, PartialEq)]");
        out.wln(&format!("pub enum {name}"));
        {
            let mut idt = out.indent();
            for m in &variant.members {
                let case_name = variant_case_name(ctx, m)?;
                let payload = if matches!(ctx.lookup(m.ty).kind, TypeKind::Void) {
                    None
                } else {
                    Some(ctx.name_of(m.ty))
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

    {
        let has_ref_payload = variant
            .members
            .iter()
            .any(|m| !matches!(ctx.lookup(m.ty).kind, TypeKind::Void));
        let needs_lt = has_ref_payload || ctx.view_needs_lifetime(id);
        let lt = if needs_lt { "<'a>" } else { "" };
        out.wln("#[allow(non_camel_case_types)]");
        out.wln("#[derive(Debug, Clone, Copy, PartialEq)]");
        out.wln(&format!("pub enum {view_type}{lt}"));
        {
            let mut idt = out.indent();
            for m in &variant.members {
                let case_name = variant_case_viewname(ctx, m)?;
                let payload = if matches!(ctx.lookup(m.ty).kind, TypeKind::Void) {
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

    out.newline();

    //
    reader(ctx, out, id, variant)?;

    writer(ctx, out, id, variant)?;

    Ok(())
}

// Keep variant arm names stable and unique even when payload types repeat.
/// Computes a stable, unique variant case name.
fn variant_case_name(ctx: &RustContext, m: &VariantMember) -> Result<String> {
    let base = ctx.name_of(m.ty);
    Ok(format!("{base}_{}", m.value))
}

// Keep variant arm names stable and unique even when payload types repeat.
/// Computes a stable, unique variant case name.
fn variant_case_viewname(ctx: &RustContext, m: &VariantMember) -> Result<String> {
    let base = ctx.rust_view_type(m.ty);
    Ok(format!("{base}_{}", m.value))
}

fn reader(ctx: &RustContext, out: &mut impl Sink, id: TypeID, variant: &Variant) -> Result<()> {
    let name = ctx.name_of(id);

    emit_jread(&name, None, out, |idt| {
        idt.wln(&format!(
            "let tag = {};",
            read_expr(ctx, variant.discriminant)
        ));
        idt.wln("match tag");
        {
            let mut mtch = idt.indent();
            for m in &variant.members {
                let case_name = variant_case_name(ctx, m)?;
                mtch.wln(&format!("{} => ", m.value));
                {
                    let mut body = mtch.indent();

                    let mty = if matches!(ctx.lookup(m.ty).kind, TypeKind::Void) {
                        "()".to_string()
                    } else {
                        ctx.name_of(m.ty)
                    };

                    if mty == "()" {
                        body.wln(&format!("Ok({name}::{case_name})"));
                    } else {
                        body.wln(&format!("let payload = {};", read_expr(ctx, m.ty)));
                        body.wln(&format!("Ok({name}::{case_name}(payload))"));
                    }
                }
                mtch.wln(",");
            }
            mtch.wln(&format!(
                "_ => Err(invalid_data(format!(\"unknown tag for {name}: {{tag}}\"))),"
            ));
        }
        Ok(())
    })
}

fn writer(ctx: &RustContext, out: &mut impl Sink, id: TypeID, variant: &Variant) -> Result<()> {
    let plain_type = ctx.name_of(id);
    let view_type = ctx.rust_view_type(id);

    emit_jwrite(&plain_type, None, out, |idt| {
        //let write_tag = write_primitive_method(ctx, variant.discriminant)?;
        let disc_type = ctx.name_of(variant.discriminant);
        idt.wln("match self");
        {
            let mut mtch = idt.indent();
            for m in &variant.members {
                let case_name = variant_case_name(ctx, m)?;
                let is_void = matches!(ctx.lookup(m.ty).kind, TypeKind::Void);

                if is_void {
                    mtch.wln(&format!("{plain_type}::{case_name} =>"));
                    {
                        let mut body = mtch.indent();
                        let expr = write_expr(&format!("({}{})", m.value, disc_type));
                        body.wln(&format!("{expr};"));
                        body.wln("Ok(())");
                    }
                    mtch.wln(",");
                } else {
                    mtch.wln(&format!("{plain_type}::{case_name}(inner) =>"));
                    {
                        let mut body = mtch.indent();
                        let expr = write_expr(&format!("({}{})", m.value, disc_type));
                        body.wln(&format!("{expr};"));
                        let expr = write_expr("inner");
                        body.wln(&format!("{expr};"));
                        body.wln("Ok(())");
                    }
                    mtch.wln(",");
                }
            }
        }
        Ok(())
    })?;

    emit_jwrite(&view_type, Some("'a"), out, |idt| {
        //let write_tag = write_primitive_method(ctx, variant.discriminant)?;
        let disc_type = ctx.name_of(variant.discriminant);
        idt.wln("match self");
        {
            let mut mtch = idt.indent();
            for m in &variant.members {
                let case_name = variant_case_viewname(ctx, m)?;
                let is_void = matches!(ctx.lookup(m.ty).kind, TypeKind::Void);

                if is_void {
                    mtch.wln(&format!("{view_type}::{case_name} =>"));
                    {
                        let mut body = mtch.indent();
                        let expr = write_expr(&format!("({}{})", m.value, disc_type));
                        body.wln(&format!("{expr};"));
                        body.wln("Ok(())");
                    }
                    mtch.wln(",");
                } else {
                    mtch.wln(&format!("{view_type}::{case_name}(inner) =>"));
                    {
                        let mut body = mtch.indent();
                        let expr = write_expr(&format!("({}{})", m.value, disc_type));
                        body.wln(&format!("{expr};"));
                        let expr = write_expr("inner");
                        body.wln(&format!("{expr};"));
                        body.wln("Ok(())");
                    }
                    mtch.wln(",");
                }
            }
        }
        Ok(())
    })
}
