use crate::{
    codegen::{
        Sink,
        rust::{context::RustContext, util::*},
    },
    compile::{Pack, Type, TypeID},
};

use anyhow::Result;

pub fn emit_pack(
    ctx: &RustContext,
    out: &mut impl Sink,
    id: TypeID,
    _ty: &Type,
    pack: &Pack,
) -> Result<()> {
    // bitfields only have one form
    let name = ctx.name_of(id);

    out.wln("#[repr(C, packed(1))]");
    out.wln(&format!("pub struct {name}"));
    {
        let mut idt = out.indent();
        for m in &pack.members {
            let field_ty = ctx.name_of(m.ty);
            idt.wln(&format!("pub {}: {},", m.name, field_ty));
        }
    }

    out.wln(&format!("impl std::fmt::Debug for {name}"));
    {
        let mut idt = out.indent();
        idt.wln("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result");
        {
            let mut idt = idt.indent();

            for item in &pack.members {
                idt.wln(&format!(
                    "let {0} = unsafe {{  (&raw const self.{0}).read_unaligned() }};",
                    item.name
                ));
            }

            idt.wln(&format!("f.debug_struct(\"{name}\")"));

            for item in &pack.members {
                idt.wln(&format!(".field(\"{0}\", &{0})", item.name));
            }

            idt.wln(".finish()");
        }
    }

    out.wln(&format!("impl Clone for {name}"));
    {
        let mut idt = out.indent();
        idt.wln("fn clone(&self) -> Self");
        {
            let mut idt = idt.indent();

            idt.wln("*bytemuck::from_bytes(bytemuck::bytes_of(self))");
        }
    }

    out.wln(&format!("impl PartialEq for {name}"));
    {
        let mut idt = out.indent();
        idt.wln("fn eq(&self, other: &Self) -> bool");
        {
            let mut idt = idt.indent();

            let mut mem_check = vec![];

            for item in &pack.members {
                mem_check.push(format!("{{self.{0}}} == {{other.{0}}}", item.name));
            }

            idt.wln(&(String::from("return ") + &mem_check.join("&&")));
        }
    }

    out.wln(&format!("impl Copy for {name} {{ }}"));

    out.wln(&format!("unsafe impl bytemuck::Zeroable for {name} {{ }}"));
    out.wln(&format!("unsafe impl bytemuck::Pod for {name} {{ }}"));
    out.wln(&format!("impl Default for {name}"));
    {
        let mut idt = out.indent();
        idt.wln("fn default() -> Self");
        {
            let mut body = idt.indent();
            body.wln(&format!("<{name} as bytemuck::Zeroable>::zeroed()"));
        }
    }

    out.newline();

    //
    reader(out, &name)?;

    writer(out, &name)?;

    Ok(())
}

fn reader(out: &mut impl Sink, name: &str) -> Result<()> {
    emit_jread(name, None, out, |idt| {
        idt.wln("// Safety: Bytes will be overwritten anyway");
        idt.wln("#[allow(invalid_value)]");
        idt.wln("let mut tmp = unsafe { std::mem::MaybeUninit::uninit().assume_init() };");
        idt.wln("reader.read_exact(bytemuck::bytes_of_mut(&mut tmp))?;");
        idt.wln("Ok(tmp)");
        Ok(())
    })
}

fn writer(out: &mut impl Sink, name: &str) -> Result<()> {
    emit_jwrite(name, None, out, |idt| {
        idt.wln("writer.write_all(bytemuck::bytes_of(self))");
        Ok(())
    })
}
