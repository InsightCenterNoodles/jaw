use std::io::Write;

use thiserror::Error;

use crate::module::Module;

mod cpp;
mod python;
mod common;
mod rust;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum KnownGenerators {
    CPP,
    PYTHON,
    RUST,
}

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("IO error")]
    IO(#[from] std::io::Error),
}

pub fn emit_for(
    ty: KnownGenerators,
    module: Module,
    file: impl Write,
) -> Result<(), GeneratorError> {
    match ty {
        KnownGenerators::CPP => cpp::emit_cpp_header(module, file),
        KnownGenerators::PYTHON => python::emit_python(module, file),
        KnownGenerators::RUST => rust::emit_rust(module, file),
    }
}
