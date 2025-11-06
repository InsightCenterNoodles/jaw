use std::io::Write;

use crate::module::Module;

mod cpp;
mod python;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum KnownGenerators {
    CPP,
}

pub fn emit_for(ty: KnownGenerators, module: Module, file: impl Write) -> std::io::Result<()> {
    match ty {
        KnownGenerators::CPP => cpp::emit_cpp_header(module, file),
    }
}
