# jaw

A binary protocol code generator. A small DSL (`.jaw`) is used to describe binary message layouts. Code generation is provided for C++, GDScript, JavaScript, Python, Rust and Swift.

Quickstart

- Build: `cargo build`
- Run: `cargo run -- --kind <cpp|gdscript|javascript|python|rust|swift> <OUTPUT> <INPUT.jaw>...`
  - Example (Python): `cargo run -- --kind python /tmp/example.py assets/example.jaw`
  - Example (C++): `cargo run -- --kind cpp /tmp/example.hpp assets/example.jaw`
  - Example (GDScript): `cargo run -- --kind gdscript /tmp/example.gd assets/example.jaw`
  - Example (JavaScript): `cargo run -- --kind javascript /tmp/example.js assets/example.jaw`
  - Example (Rust): `cargo run -- --kind rust /tmp/example.rs assets/example.jaw`
  - Example (Swift): `cargo run -- --kind swift /tmp/example.swift assets/example.jaw`

The output path comes before one or more input schemas. When multiple inputs are supplied, their definitions are merged into one generated module.

Benchmarks

- Run the generated Rust encode/decode benchmarks: `cargo bench --bench generated_rust`
- The benchmarks use checked-in Rust generator output and report latency and byte throughput for small messages, nested messages, and byte/POD arrays from empty through roughly 1 MiB. They also compare reused, fresh, and buffered writers.
- Run the optimized Swift benchmarks on macOS:
  1. `cargo run -- --kind swift /tmp/jaw_swift_benchmark.swift benches/swift/benchmark.jaw`
  2. `swiftc -O -swift-version 6 /tmp/jaw_swift_benchmark.swift benches/swift/main.swift -o /tmp/jaw_swift_benchmark`
  3. `/tmp/jaw_swift_benchmark`
- The Swift benchmark covers 1 MiB byte and float arrays, fixed-size record arrays, nested messages, buffered streams, and reused writers.

DSL Summary

An example of the available features is provided in `assets/example.jaw`.

- Primitives: `bool,u8,u16,u32,u64,i8,i16,i32,i64,f32,f64`
  - `bool` occupies one wire byte and accepts only `0` or `1` in ordinary values
  - Boolean constants use the literals `true` and `false`
  - Packed/POD booleans use a `WireBool` byte wrapper and validate when accessed, allowing packs to be forwarded without a recursive validation pass
  - Bitfield members remain integer-valued; a one-bit member is still declared as `u8`
  - `WireBool` is reserved for the generated packed representation
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
  - `variant Name : <primitive-int>` alts `- CaseName = <int> => Type` and reservations `- reserve <int>[..=<int>]`
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
- GDScript: a single preloaded script for Godot 4.7 with typed messages and a `PackedByteArray` API.
- JavaScript: a dependency-free ES module for modern browsers and Node.js. Byte arrays use
  `Uint8Array`; 64-bit integers use `BigInt`.
- Rust: a module with types plus `read_<Type>`/`write_<Type>` functions using `std::io::Read/Write`.
- Swift: Foundation-based value types implementing `JawReadable`/`JawWritable`, with `Data` and stream adapters.

Swift generation targets Swift 6.2 and macOS. `--swift-types-as-data <TYPE>...` maps selected
dynamic `u8` arrays to `Foundation.Data`. `--swift-share-protocols <MODULE>` imports the
complete Jaw Swift runtime from a shared module instead of emitting it into the output.
`JawOutputStream` buffers writes and must be explicitly flushed with `flush()` before its
underlying stream is consumed or closed.

### Godot 4.7

Place the generated `.gd` file in the project and preload it. Schema types and constants use a
`Jaw_` prefix to avoid clashes with Godot's built-in classes. Fields that collide with GDScript
identifiers are escaped. In this example, JAW's `var` field becomes `var_`.

```gdscript
const Messages = preload("res://example.gd")

func make_packet() -> PackedByteArray:
    var packet := Messages.Jaw_Root.new()
    packet.name = Messages.Jaw_ShortString.new("ready".to_utf8_buffer())
    packet.var_ = Messages.Jaw_MyVariant.make_Empty()
    var encoded := packet.encode()
    if encoded.error != OK:
        push_error(encoded.message)
        return PackedByteArray()
    return encoded.value

func receive_packet(bytes: PackedByteArray) -> void:
    var decoded := Messages.Jaw_Root.decode(bytes)
    if decoded.error != OK:
        push_warning("Byte %d: %s" % [decoded.offset, decoded.message])
        return
    var packet: Messages.Jaw_Root = decoded.value
    print(packet.name.values.get_string_from_utf8())
```

- `encode()` and `decode(bytes)` return a `JawResult` with `error`, `offset` and `message`.
  On success, `error` is `OK` and `value` contains the bytes or decoded object.
  `decode()` expects exactly one value and rejects trailing bytes.
- Array wrappers expose `values` as a `PackedByteArray` for `u8` elements and a typed `Array`
  for other elements. Enum wrappers expose `raw_value`, `VALUE_…` constants, and `member_name()`.
  Variant wrappers expose `tag`, `payload`, `TAG_…` constants, and `make_…()` factories.
- `u64` uses a signed Godot `int`. Values with the high bit set appear negative and retain all
  64 wire bits. Packed booleans retain their raw byte in `WireBool` and validate on
  `get_value()`, which also returns a `JawResult`.
- `--guard-array-size` limits array decoding. The codec also limits message nesting and
  arrays of zero-byte elements.

Run the headless compatibility test with `cargo test --test gdscript`. Set `GODOT_BIN` to the
Godot 4.7 executable if it is outside the standard locations.
