use crate::{module::*, tokens::Span};
use std::fmt::Write;

pub fn emit_python(
    module: Module,
    mut file: impl std::io::Write,
) -> Result<(), super::GeneratorError> {
    todo!()
}
