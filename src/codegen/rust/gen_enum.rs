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
    let name = ctx.name_of(id);
    let value_name = format!("{name}Value");
    let base = ctx.name_of(kind.underlying);

    out.wln("#[repr(transparent)]");
    out.wln("#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]");
    out.wln(&format!("pub struct {name}({base});"));
    out.wln(&format!("unsafe impl bytemuck::Zeroable for {name} {{}}"));
    out.wln(&format!("unsafe impl bytemuck::Pod for {name} {{}}"));
    out.newline();

    out.wln(&format!("#[repr({base})]"));
    out.wln("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
    out.wln(&format!("pub enum {value_name}"));
    {
        let mut body = out.indent();
        for member in &kind.members {
            body.wln(&format!("{} = {},", member.name, member.value));
        }
    }
    out.newline();

    out.wln(&format!("impl {name}"));
    {
        let mut body = out.indent();
        for member in &kind.members {
            body.wln(&format!(
                "pub const {}: Self = Self({});",
                member.name, member.value
            ));
        }
        body.wln(&format!(
            "pub const fn from_raw(raw: {base}) -> Self {{ Self(raw) }}"
        ));
        body.wln(&format!("pub const fn raw(self) -> {base} {{ self.0 }}"));
        body.wln(&format!(
            "pub fn value(self) -> std::io::Result<{value_name}>"
        ));
        {
            let mut value_body = body.indent();
            value_body.wln("match self.0");
            {
                let mut arms = value_body.indent();
                for member in &kind.members {
                    arms.wln(&format!(
                        "{} => Ok({value_name}::{}),",
                        member.value, member.name
                    ));
                }
                arms.wln(&format!(
                    "raw => Err(invalid_data(format!(\"invalid value for {name}: {{raw}}\"))),"
                ));
            }
        }
    }
    out.newline();

    out.wln(&format!("impl From<{value_name}> for {name}"));
    {
        let mut body = out.indent();
        body.wln(&format!("fn from(value: {value_name}) -> Self"));
        {
            let mut function = body.indent();
            function.wln(&format!("Self(value as {base})"));
        }
    }
    out.newline();

    emit_jread(&name, None, out, |body| {
        body.wln(&format!("Ok(Self({}))", read_expr(ctx, kind.underlying)));
        Ok(())
    })?;

    emit_jwrite(&name, None, out, |body| {
        body.wln(&format!("{};", write_expr("self.0")));
        body.wln("Ok(())");
        Ok(())
    })?;

    Ok(())
}
