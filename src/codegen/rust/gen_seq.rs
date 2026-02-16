use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Sequence, Type, TypeID},
};

use anyhow::Result;

pub fn emit_seq(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    sequence: &Sequence,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);

    let view_type = ctx.rust_view_type(id);

    {
        out.wln("#[derive(Debug, Clone, PartialEq)]");
        out.wln(&format!("pub struct {name}"));
        {
            let mut idt = out.indent();
            for m in &sequence.members {
                let field_ty = ctx.name_of(m.ty);
                idt.wln(&format!("pub {}: {},", m.name, field_ty));
            }
        }
    }

    {
        let lt = if ctx.view_needs_lifetime(id) {
            "<'a>"
        } else {
            ""
        };
        out.wln("#[derive(Debug, PartialEq)]");
        out.wln(&format!("pub struct {view_type}{lt}"));
        {
            let mut idt = out.indent();
            for m in &sequence.members {
                let field_ty = ctx.rust_write_decl_type(m.ty, "'a")?;
                idt.wln(&format!("pub {}: {},", m.name, field_ty));
            }
        }
        out.newline();
    }

    out.newline();

    //
    reader(ctx, out, id, sequence)?;

    writer(ctx, out, id, sequence)?;

    Ok(())
}

fn reader(ctx: &RustContext, out: &mut impl Sink, id: TypeID, sequence: &Sequence) -> Result<()> {
    let name = ctx.name_of(id);

    emit_jread(&name, None, out, |idt| {
        for m in &sequence.members {
            let expr = read_expr(ctx, m.ty);
            idt.wln(&format!("let {} = {expr};", m.name));
        }
        idt.wln(&format!("Ok({name}"));
        {
            let mut body = idt.indent();
            for m in &sequence.members {
                body.wln(&format!("{},", m.name));
            }
        }
        idt.wln(")");
        Ok(())
    })
}

fn writer(ctx: &RustContext, out: &mut impl Sink, id: TypeID, sequence: &Sequence) -> Result<()> {
    let plain_type = ctx.name_of(id);
    let view_type = ctx.rust_view_type(id);

    emit_jwrite(&plain_type, None, out, |idt| {
        for m in &sequence.members {
            let expr = write_expr(&format!("self.{}", m.name));
            let expr = &format!("{expr};");
            idt.wln(&expr);
        }
        idt.wln("Ok(())");
        Ok(())
    })?;

    let lt = ctx.view_needs_lifetime(id).then(|| "'a");

    emit_jwrite(&view_type, lt, out, |idt| {
        for m in &sequence.members {
            let expr = write_expr(&format!("self.{}", m.name));
            let expr = &format!("{expr};");
            idt.wln(&expr);
        }
        idt.wln("Ok(())");
        Ok(())
    })
}
