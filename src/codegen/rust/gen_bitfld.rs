use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Bitfld, Type, TypeID, TypeKind},
};

use anyhow::Result;

pub fn emit_bitfld(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    kind: &Bitfld,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);
    let base = ctx.name_of(kind.underlying);

    out.wln("#[derive(Debug, Clone, PartialEq)]");
    out.wln(&format!("pub struct {name}"));
    {
        let mut idt = out.indent();
        for m in &kind.members {
            let field_ty = ctx.name_of(m.underlying);
            idt.wln(&format!("pub {}: {},", m.name, field_ty));
        }
    }
    out.newline();

    //
    reader(ctx, out, id, kind, &name)?;

    emit_jwrite(&name, None, out, |idt| {
        idt.wln("let mut raw: u64 = 0;");
        for m in &kind.members {
            let start = m.range.start();
            let end = m.range.end();
            let width = end - start + 1;
            let mask = (1u128 << width) - 1;
            idt.wln(&format!(
                "raw |= ((self.{} as u64) & 0x{mask:X}) << {start};",
                m.name
            ));
        }
        idt.wln(&format!("let raw = raw as {};", base));
        idt.wln(&format!("{};", write_expr("raw")));
        idt.wln("Ok(())");
        Ok(())
    })?;

    Ok(())
}

fn reader(
    ctx: &RustContext,
    out: &mut impl Sink,
    _id: TypeID,
    kind: &Bitfld,
    name: &String,
) -> Result<()> {
    let base_read = read_expr(ctx, kind.underlying);

    emit_jread(name, None, out, |idt| {
        idt.wln(&format!("let raw = {base_read};"));
        for m in &kind.members {
            let start = m.range.start();
            let end = m.range.end();
            let width = end - start + 1;
            let mask = (1u128 << width) - 1;
            let field = &m.name;
            let field_ty = ctx.lookup(m.underlying);
            let assign_expr = match &field_ty.kind {
                TypeKind::Enum(enm) => {
                    let mut s = String::new();
                    s.push_str(&format!(
                        "let {field}_raw = ((raw >> {start}) & 0x{mask:X}) as {};\n",
                        ctx.name_of(enm.underlying)
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
                            variant = mem.name
                        ));
                    }
                    if let Some(default) = &enm.default {
                        s.push_str(&format!(
                            "    _ => {enm_name}::{variant},\n",
                            enm_name = ctx.name_of(m.underlying),
                            variant = default.name
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
                    ctx.name_of(m.underlying),
                    ctx.name_of(m.underlying)
                ),
            };
            for line in assign_expr.lines() {
                idt.wln(line);
            }
        }
        idt.wln(&format!("Ok({name}"));
        {
            let mut args = idt.indent();
            for m in &kind.members {
                args.wln(&format!("{},", m.name));
            }
        }
        idt.wln(")");
        Ok(())
    })
}
