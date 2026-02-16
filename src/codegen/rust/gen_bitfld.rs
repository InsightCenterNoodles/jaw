use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Bitfld, BitfldMember, Type, TypeID, TypeKind},
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

    out.wln("#[derive(Debug, Default, Clone, Copy, PartialEq)]");
    out.wln("#[repr(transparent)]");
    out.wln(&format!("pub struct {name}({base});"));
    out.newline();

    out.wln(&format!("impl {name}"));
    {
        let mut idt = out.indent();
        for m in &kind.members {
            emit_getter(ctx, &mut idt, m);
            emit_setter(ctx, &mut idt, m, &base);
        }
    }
    out.newline();

    out.wln("#[derive(Debug, PartialEq)]");
    out.wln(&format!("pub struct {name}Unpack"));
    {
        let mut idt = out.indent();
        for m in &kind.members {
            let field_ty = ctx.name_of(m.underlying);
            idt.wln(&format!("pub {}: {},", m.name, field_ty));
        }
    }
    out.newline();

    //

    emit_convert(ctx, out, id, kind);
    out.newline();

    //

    emit_jread(&name, None, out, |idt| {
        idt.wln(&format!("Ok(Self({}))", read_expr(ctx, kind.underlying)));
        Ok(())
    })?;

    emit_jwrite(&name, None, out, |idt| {
        idt.wln(&format!("{};", write_expr("self.0")));
        idt.wln("Ok(())");
        Ok(())
    })?;

    Ok(())
}

fn emit_convert(ctx: &RustContext, out: &mut impl Sink, id: TypeID, kind: &Bitfld) {
    let name = ctx.name_of(id);

    out.wln(&format!("impl From<{name}Unpack> for {name}"));

    {
        let mut idt = out.indent();

        idt.wln(&format!("fn from(value: {name}Unpack) -> Self"));

        {
            let mut idt = idt.indent();

            idt.wln("let mut ret = Self::default();");

            for m in &kind.members {
                idt.wln(&format!("ret.set_{0}(value.{0});", m.name));
            }

            idt.wln("ret");
        }
    }

    out.wln(&format!("impl TryFrom<{name}> for {name}Unpack"));

    {
        let mut idt = out.indent();

        idt.wln("type Error = std::io::Error;");

        idt.wln(&format!(
            "fn try_from(value: {name}) -> Result<Self, Self::Error>"
        ));

        {
            let mut idt = idt.indent();

            idt.wln("Ok(Self");

            {
                let mut idt = idt.indent();

                for m in &kind.members {
                    if let TypeKind::Enum(_) = ctx.lookup(m.underlying).kind {
                        idt.wln(&format!("{0} : value.{0}()?,", m.name));
                    } else {
                        idt.wln(&format!("{0} : value.{0}(),", m.name));
                    }
                }
            }

            idt.wln(")");
        }
    }
}

fn emit_getter(ctx: &RustContext, out: &mut impl Sink, m: &BitfldMember) {
    let field_ty = ctx.name_of(m.underlying);

    match &ctx.lookup(m.underlying).kind {
        TypeKind::Enum(_) => out.wln(&format!(
            "pub fn {}(&self) -> std::io::Result<{}>",
            m.name, field_ty
        )),
        _ => {
            out.wln(&format!("pub fn {}(&self) -> {}", m.name, field_ty));
        }
    }

    {
        let mut idt = out.indent();
        //
        let start = m.range.start();
        let end = m.range.end();
        let width = end - start + 1;
        let mask = (1u128 << width) - 1;
        //let field = &m.name;
        let field_ty = ctx.lookup(m.underlying);
        match &field_ty.kind {
            TypeKind::Enum(enm) => {
                idt.wln(&format!(
                    "let raw = ((self.0 >> {start}) & 0x{mask:X}) as {};\n",
                    ctx.name_of(enm.underlying)
                ));
                idt.wln("let ret = match raw");

                {
                    let mut idt = idt.indent();

                    for mem in &enm.members {
                        idt.wln(&format!(
                            "{} => {}::{},",
                            mem.value,
                            ctx.name_of(m.underlying),
                            mem.name
                        ));
                    }
                    if let Some(default) = &enm.default {
                        idt.wln(&format!(
                            "_ => {}::{},",
                            ctx.name_of(m.underlying),
                            default.name
                        ));
                    } else {
                        idt.wln(
                            "_ => return Err(invalid_data(\"invalid discriminant in bitfield\")),",
                        );
                    }
                }

                idt.wln(";Ok(ret)");
            }
            _ => idt.wln(&format!(
                "((self.0 >> {start}) & 0x{mask:X}) as {}",
                ctx.name_of(m.underlying)
            )),
        };
    }
}

fn emit_setter(ctx: &RustContext, out: &mut impl Sink, m: &BitfldMember, base: &str) {
    let field_ty = ctx.name_of(m.underlying);

    out.wln(&format!("pub fn set_{}(&mut self, v: {field_ty})", m.name));

    {
        let mut idt = out.indent();

        let start = m.range.start();
        let end = m.range.end();
        let width = end - start + 1;
        let mask = (1u128 << width) - 1;

        if let TypeKind::Enum(_) = &ctx.lookup(m.underlying).kind {
            //let pname = ctx.name_of(enum_type.underlying);
            idt.wln(&format!("let v = v as {base};"));
        }

        idt.wln(&format!("self.0 |= (v & 0x{mask:X}) << {start};"));
    }
}
