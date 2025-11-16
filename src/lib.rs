//! jaw: a small DSL for describing binary message layouts and generating code.
//!
//! Crate layout
//! - `tokens`: the tokenizer (lexer) that turns source text into tokens with spans
//! - `tokenreader`: thin helper for consuming tokens ergonomically in the parser
//! - `module`: parser + validation that builds a typed intermediate representation
//! - `codegen`: backends for C++, Python, and Rust
//!
//! Most consumers interact with the CLI binary, but the `prelude` below exposes
//! common types for embedding.

mod codegen;
mod module;
mod reader;
mod tokenreader;
mod tokens;

pub mod prelude {
    pub use super::tokens::LexError;
    pub use super::tokens::Span;

    pub use super::module::Module;
    pub use super::module::ModuleBuildError;
    pub use super::module::PartialModule;

    pub use super::codegen::GeneratorError;
    pub use super::codegen::KnownGenerators;
    pub use super::codegen::emit_for;
}
