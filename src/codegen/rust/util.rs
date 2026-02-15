use crate::{
    codegen::{Indenter, Sink, rust::context::RustContext},
    compile::{TypeID, TypeKind},
};

use anyhow::Result;

pub fn emit_jread<Func: FnOnce(&mut Indenter<'_>) -> Result<()>>(
    tname: &str,
    lifetime: Option<&str>,
    out: &mut impl Sink,
    f: Func,
) -> Result<()> {
    let lifetime = lifetime.map(|x| format!("<{x}>")).unwrap_or_default();
    out.wln(&format!("impl{0} JawRead for {tname}{0}", lifetime));
    {
        let mut idt = out.indent();

        idt.wln("#[inline]");
        idt.wln("fn jaw_read<R: Read>(reader: &mut R) -> io::Result<Self>");
        {
            let mut idt = idt.indent();

            f(&mut idt)?;
        }
    }
    Ok(())
}

pub fn emit_jwrite<Func: FnOnce(&mut Indenter<'_>) -> Result<()>>(
    tname: &str,
    lifetime: Option<&str>,
    out: &mut impl Sink,
    f: Func,
) -> Result<()> {
    let lifetime = lifetime.map(|x| format!("<{x}>")).unwrap_or_default();
    out.wln(&format!("impl{0} JawWrite for {tname}{0}", lifetime));
    {
        let mut idt = out.indent();

        idt.wln("#[inline]");
        idt.wln("fn jaw_write<W: Write>(&self, writer: &mut W) -> io::Result<()>");
        {
            let mut idt = idt.indent();

            f(&mut idt)?;
        }
    }

    Ok(())
}

/// Builds a Rust expression string to read a value of `id` from `reader_ident`.
#[must_use]
pub fn read_expr(ctx: &RustContext, id: TypeID) -> String {
    let ty = ctx.lookup(id);
    match &ty.kind {
        TypeKind::Void => "Ok(())".into(),
        _ => format!("{}::jaw_read(reader)?", ctx.name_of(id)),
    }
}

/// Emits Rust statements that write `value_expr` of type `id` into `writer`.
#[must_use]
pub fn write_expr(value_expr: &str) -> String {
    format!("{value_expr}.jaw_write(writer)?")
}
