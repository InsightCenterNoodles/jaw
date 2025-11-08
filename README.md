# jaw
A binary protocol code generator

Overview
- Parse a small DSL (`.jaw`) that describes binary message layouts
- Validate and compile into an intermediate type graph
- Generate reader/writer code for C++ or Python

Usage
- Build: `cargo build` (Rust 2024 edition)
- Run: `cargo run -- <INPUT.jaw> --kind <CPP|PYTHON> <OUTPUT>`
  - Example (Python): `cargo run -- assets/basic.jaw --kind PYTHON generated/python/basic.py`
  - Example (C++): `cargo run -- assets/basic.jaw --kind CPP generated/cpp/src/basic.hpp`

DSL Summary
- Primitives: `u8,u16,u32,u64,i8,i16,i32,i64,f32,f64`
- Pack (POD struct):
  - `pack Name` then indented members `- field : Type`
  - All members must be POD (primitives, enums, other packs, fixed arrays)
- Enum (integer base with optional default):
  - `enum Name : <primitive-int>` with members `- NAME = <int>` and optional `= DEFAULT = <int>`
- Bitfield (on integer base):
  - `bits Name : <primitive-int>` then members `- start[-end]  field : <primitive|enum>`
- Sequence (dynamic, decoded field by field):
  - `seq Name` with members `- field : Type`
- Variant (tagged union):
  - `variant Name : <primitive-int>` with alts `- <int> => Type` and optional default `= <int> => Type`
  - On-wire discriminant is the explicit `<int>`; generated C++ maps this correctly to `std::variant` alternatives
- Arrays:
  - Dynamic: `{N * M}` reads count of type `N` then `M` items (sequence field)
  - Fixed dynamic-count: `{I * M}` literal `I` items (sequence field)
  - Fixed POD: `[I * M]` literal `I` items; POD-only and used inside packs

Examples
- See `assets/basic.jaw` and generated outputs under `generated/` for reference.

Notes
- Aliases and imports (`alias`, `use/as`) are reserved but not implemented yet.
- Endianness is little-endian in the Python generator; C++ expects the Reader/Writer to define endianness.
