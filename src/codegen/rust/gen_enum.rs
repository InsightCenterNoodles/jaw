use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Enum, Type, TypeID},
};

use anyhow::Result;

pub fn emit_enum(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    kind: &Enum,
) -> Result<()> {
    // Enums only have one form
    let name = ctx.name_of(id);
    let base = ctx.name_of(kind.underlying);
    out.wln(&format!("#[repr({base})]"));
    out.wln("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
    out.wln(&format!("pub enum {name}"));
    {
        let mut idt = out.indent();
        for m in &kind.members {
            idt.wln(&format!("{} = {},", m.name, m.value));
        }
        if let Some(default) = &kind.default {
            idt.wln(&format!("{} = {},", default.name, default.value));
        }
    }
    out.newline();

    //

    let base_read = read_expr(ctx, kind.underlying);

    emit_jread(&name, None, out, |idt| {
        idt.wln(&format!("let raw = {base_read};"));
        idt.wln("match raw");
        {
            let mut mtch = idt.indent();
            for m in &kind.members {
                mtch.wln(&format!("{} => Ok({}::{}),", m.value, name, m.name));
            }
            if let Some(default) = &kind.default {
                mtch.wln(&format!("_ => Ok({}::{}),", name, default.name));
            } else {
                mtch.wln(&format!(
                    "_ => Err(invalid_data(format!(\"invalid discriminant for {name}: {{raw}}\"))),"
                ));
            }
        }
        Ok(())
    })?;

    emit_jwrite(&name, None, out, |idt| {
        idt.wln(&format!("let raw : {base} = *self as _;"));
        idt.wln(&format!("{};", write_expr("raw")));
        idt.wln("Ok(())");
        Ok(())
    })?;

    Ok(())
}
