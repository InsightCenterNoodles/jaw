# jaw

A binary protocol code generator. A small DSL (`.jaw`) is used to describe binary message layouts. Code generation is provided for C++, Python and Rust.

Quickstart

- Build: `cargo build`
- Run: `cargo run -- --kind <cpp|python|rust> <OUTPUT> <INPUT.jaw>...`
  - Example (Python): `cargo run -- --kind python /tmp/example.py assets/example.jaw`
  - Example (C++): `cargo run -- --kind cpp /tmp/example.hpp assets/example.jaw`
  - Example (Rust): `cargo run -- --kind rust /tmp/example.rs assets/example.jaw`

The output path comes before one or more input schemas. When multiple inputs are supplied, their definitions are merged into one generated module.

Benchmarks

- Run the generated Rust encode/decode benchmarks: `cargo bench --bench generated_rust`
- The benchmarks use checked-in Rust generator output and report latency and byte throughput for small messages, nested messages, and byte/POD arrays from empty through roughly 1 MiB. They also compare reused, fresh, and buffered writers.

DSL Summary

An example of the available features is provided in `assets/example.jaw`.

- Primitives: `u8,u16,u32,u64,i8,i16,i32,i64,f32,f64`
- Pack (POD, packed, struct):
  - `pack Name` then members as `- field : Type`
  - Members must be POD (primitives, enums, packs, fixed arrays)
- Enum (integer-backed, converted to named values on demand):
  - `enum Name : <primitive-int>` members `- NAME = <int>` and reservations `- reserve <int>[..=<int>]`
- Bitfield (on integer base):
  - `bits Name : <primitive-int>` then `- start[-end]  field : <primitive|enum>`
- Sequence (dynamic, decoded field by field):
  - `seq Name` with `- field : Type`
  - A sequence may have no members; it is a named empty type with zero wire size
- Variant (tagged union):
  - `variant Name : <primitive-int>` alts `- <int> => Type` and reservations `- reserve <int>[..=<int>]`
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

- Python: reader/writer helpers and simple data structures. See `assets/generated/python/` examples.
- C++: header-only types and readers/writers.
- Rust: a module with types plus `read_<Type>`/`write_<Type>` functions using `std::io::Read/Write`.
