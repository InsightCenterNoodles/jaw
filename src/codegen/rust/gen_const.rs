use crate::{
    codegen::Sink,
    codegen::rust::context::RustContext,
    compile::{Const, PrimitiveLiteral, Type, TypeID},
};

use anyhow::{Result, bail};

pub fn emit_const(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    c: &Const,
) -> Result<()> {
    let name = ctx.name_of(id).to_ascii_uppercase();
    if ctx.underlying_primitive(c.ty).is_none() {
        bail!("const {name} target must be primitive-compatible");
    };
    let ty_name = ctx.name_of(c.ty);

    let value = match c.value {
        PrimitiveLiteral::Integer(x) => x.to_string(),
        PrimitiveLiteral::Real(x) => x.to_string(),
    };

    out.wln(&format!("pub const {name} : {ty_name} = {value};"));

    out.newline();

    Ok(())
}
