//! Code generation dispatcher for supported backends.
//!
//! This module provides a small facade (`emit_for`) that selects the
//! appropriate backend to render a single file/string for the compiled
//! `Module` IR. Backends live under `codegen::{cpp, python, rust}`.
//!
//! Backends are intentionally simple, string-based emitters that do not rely
//! on templates. They render types in topological order and generate paired
//! read/write helpers for cross-language compatibility tests.
use std::io::Write;

use thiserror::Error;

use crate::module::Module;

mod cpp;
mod python;
mod common;
mod rust;
mod swift;

/// Stable set of built-in generators exposed by the CLI.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum KnownGenerators {
    CPP,
    PYTHON,
    RUST,
    SWIFT,
}

/// Errors surfaced by code generation.
#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("IO error")]
    IO(#[from] std::io::Error),
}

/// Emit code for a compiled `Module` into the provided writer.
///
/// - `CPP` emits a single header with types and free read/write helpers.
/// - `PYTHON` emits a single `.py` module with ctypes structs and helpers.
/// - `RUST` emits a Rust module with types and `read_*/write_*` functions.
pub fn emit_for(
    ty: KnownGenerators,
    module: Module,
    file: impl Write,
) -> Result<(), GeneratorError> {
    match ty {
        KnownGenerators::CPP => cpp::emit_cpp_header(module, file),
        KnownGenerators::PYTHON => python::emit_python(module, file),
        KnownGenerators::RUST => rust::emit_rust(module, file),
        KnownGenerators::SWIFT => swift::emit_swift(module, file),
    }
}
