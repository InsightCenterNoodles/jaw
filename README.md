# jaw
A binary protocol code generator

Overview
- Parse a small DSL (`.jaw`) that describes binary message layouts
- Validate and compile into an intermediate type graph
- Generate reader/writer code for C++, Python, or Rust

Quickstart
- Build: `cargo build` (Rust 2024 edition)
- Run: `cargo run -- <INPUT.jaw> --kind <CPP|PYTHON|RUST> <OUTPUT>`
  - Example (Python): `cargo run -- assets/basic.jaw --kind PYTHON generated/python/basic.py`
  - Example (C++): `cargo run -- assets/basic.jaw --kind CPP generated/cpp/src/basic.hpp`
  - Example (Rust): `cargo run -- assets/basic.jaw --kind RUST generated/rust/basic.rs`

CLI
- `--kind`: selects output generator. Supported: `CPP`, `PYTHON`, `RUST`.
- Exit codes: non-zero on parse/validation/generation errors. Errors display spans using ariadne with helpful labels.

DSL Summary
- Primitives: `u8,u16,u32,u64,i8,i16,i32,i64,f32,f64`
- Pack (POD struct):
  - `pack Name` then members as `- field : Type`
  - Members must be POD (primitives, enums, packs, fixed arrays)
- Enum (integer base with optional default):
  - `enum Name : <primitive-int>` members `- NAME = <int>` and optional `= DEFAULT = <int>`
- Bitfield (on integer base):
  - `bits Name : <primitive-int>` then `- start[-end]  field : <primitive|enum>`
- Sequence (dynamic, decoded field by field):
  - `seq Name` with `- field : Type`
- Variant (tagged union):
  - `variant Name : <primitive-int>` alts `- <int> => Type` and optional default `= <int> => Type`
  - Discriminant is the explicit `<int>` value
- Arrays:
  - Dynamic sequence element: `{N * M}` reads a count of type `N`, then `M` items (sequence field only)
  - Fixed dynamic-count: `{I * M}` literal `I` items (sequence field only)
  - Fixed POD: `[I * M]` literal `I` items; POD-only (usable in `pack`)

Lexical Notes
- Whitespace: spaces, tabs and `\r` are ignored; newlines are significant and tokenized as `Newline`.
- Comments: `// ...` to end-of-line are skipped (newline still tokenized).
- Numbers: only unsigned integer literals are lexed; a leading `-` is a separate token used by the parser when needed.
- Strings: `"..."` with escapes `\\`, `\"`, `\n`, `\t`. Multiline and unknown escapes are errors.
- Symbols: `: - = ( ) [ ] { } *` plus a special `=>` fat arrow.

Generated Code
- Python: little-endian reader/writer helpers and simple data structures. See `generated/python/` examples.
- C++: header-only types and readers/writers. Endianness is determined by the provided Reader/Writer in C++.
- Rust: a module with types plus `read_<Type>`/`write_<Type>` functions using `std::io::Read/Write` in little-endian.

Examples
- See `assets/basic.jaw` and generated outputs under `generated/` for reference.

Developing
- Run tests: `cargo test`
- Project layout:
  - `src/tokens.rs`: tokenizer (lexer) and token definitions
  - `src/tokenreader.rs`: small helper for parser token consumption
  - `src/module.rs`: parser, validation, and type graph
  - `src/codegen/`: C++ and Python generators
  - `assets/`: example `.jaw` sources
- Style: keep changes focused and small; prefer helpful error messages with spans.

Notes
- Aliases and imports (`alias`, `use/as`) are reserved but not implemented yet.
- Endianness is little-endian in the Python generator; C++ expects the Reader/Writer to define endianness.
