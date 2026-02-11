# jaw
A binary protocol code generator. A small DSL (`.jaw`) is used to describe binary message layouts. Code generation is provided for C++, Python and Rust.

Quickstart
- Build: `cargo build`
- Run: `cargo run -- <INPUT.jaw> --kind <cpp|python|rust> <OUTPUT>`
  - Example (Python): `cargo run -- assets/basic.jaw --kind python generated/python/basic.py`
  - Example (C++): `cargo run -- assets/basic.jaw --kind cpp generated/cpp/src/basic.hpp`
  - Example (Rust): `cargo run -- assets/basic.jaw --kind rust generated/rust/basic.rs`

DSL Summary

An example of capabilities is provided in `assets/example.jaw`

- Primitives: `u8,u16,u32,u64,i8,i16,i32,i64,f32,f64`
- Pack (POD, packed, struct):
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
- Fixed array (fixed size contiguous array):
  - `fixed_array : <integer literal * Type>`
- Dynamic array (variably sized contiguous array):
  - `dyn_array : CountType * Type` reads a count of type `CountType`, then `Type` items
- Comments in the form of `# ...`
- Imports (must appear before declarations):
  - `from path/to/file.jaw use {TypeA, TypeB}`
  - `from path/to/file.jaw use *`

Generated Code
- Python: reader/writer helpers and simple data structures. See `generated/python/` examples.
- C++: header-only types and readers/writers.
- Rust: a module with types plus `read_<Type>`/`write_<Type>` functions using `std::io::Read/Write`.
