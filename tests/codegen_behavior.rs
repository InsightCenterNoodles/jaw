use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use jaw::{GlobalOptions, codegen, compile, intermediate};

fn world_from(src: &str) -> compile::World {
    let module =
        intermediate::Module::from_string("file".into(), src.into()).expect("module should parse");
    compile::compile(module).expect("module should compile")
}

fn temp_path(name: &str, ext: &str) -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("target");
    dir.push("tmp_tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.join(format!("{name}_{nanos}.{ext}"))
}

#[test]
fn guard_is_plumbed_into_all_generators() {
    let world = world_from(
        r#"
pack MyPOD
- a : u8

dyn_array Pods : u32 * MyPOD
"#,
    );
    let opts = GlobalOptions {
        guard_array_size: Some(16),
        ..Default::default()
    };

    // Rust
    let rust_out = temp_path("rust_guard_plumbed", "rs");
    codegen::emit_rust(&world, &opts, &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(
        rust_src.contains("checked_mul(std::mem::size_of::<MyPOD>() as u128)"),
        "rust should include checked byte-size limit"
    );
    assert!(
        rust_src.contains("array too large"),
        "rust should mention guard"
    );
    let _ = fs::remove_file(&rust_out);

    // Python
    let py_out = temp_path("python_guard_plumbed", "py");
    codegen::emit_python(&world, &opts, &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(
        py_src.contains("raise ValueError('array too large')"),
        "python should raise ValueError for guard"
    );
    let _ = fs::remove_file(&py_out);

    // C++
    let cpp_out = temp_path("cpp_guard_plumbed", "hpp");
    codegen::emit_cpp(&world, &opts, &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(
        cpp_src.contains("const uint64_t byte_limit = 16ULL;"),
        "cpp should include guard literal"
    );
    assert!(
        cpp_src.contains("byte_limit / elem_size"),
        "cpp guard should use division to avoid overflow"
    );
    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn rust_pack_default_is_emitted_for_bulk_arrays() {
    let world = world_from(
        r#"
pack MyPOD
- a_thing : u8
- b_thing : u64

fixed_array MyPODFixedList : 8 * MyPOD
dyn_array MyPODDynList : u16 * MyPOD
"#,
    );

    let rust_out = temp_path("rust_pack_default", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");

    assert!(
        rust_src.contains("unsafe impl bytemuck::Zeroable for MyPOD"),
        "rust should keep bytemuck impls for pack"
    );
    assert!(
        rust_src.contains("impl Default for MyPOD"),
        "rust should provide Default for pack so array fast-path compiles"
    );
    assert!(
        rust_src.contains("let mut out : [MyPOD; 8] = Default::default();"),
        "rust fixed-array read uses Default initialization"
    );
    assert!(
        rust_src.contains("vec![Default::default(); count]"),
        "rust dynamic-array bulk read uses Default initialization"
    );
    assert!(
        rust_src.contains("writer.write_all(bytemuck::cast_slice(self.0))?;"),
        "rust pack arrays should use the bulk write path"
    );

    let _ = fs::remove_file(&rust_out);
}

#[test]
fn cpp_arrays_of_pack_use_bulk_io() {
    let world = world_from(
        r#"
pack MyPOD
- a_thing : u8
- b_thing : u64

fixed_array MyPODFixedList : 8 * MyPOD
dyn_array MyPODDynList : u16 * MyPOD
"#,
    );

    let cpp_out = temp_path("cpp_fixed_pack_bulk", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");

    // Ensure we didn't fall back to element-by-element loops for a POD fixed array.
    assert!(
        cpp_src.contains("read(Reader& reader, MyPODFixedListReader& value)")
            && cpp_src.contains("return read_scalar(reader, value);"),
        "cpp fixed array of pack should use a bulk read"
    );
    assert!(
        cpp_src.contains("return write_bytes(writer, value.data(), sizeof(value));"),
        "cpp fixed array of pack should use a bulk write"
    );
    assert!(
        cpp_src.contains("return write_bytes(writer, value.data(), value.size_bytes());"),
        "cpp dynamic array of pack should use a bulk write"
    );

    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn bool_codegen_is_checked_for_values_and_lazy_for_pod() {
    let world = world_from(
        r#"
const Enabled : bool = true
fixed_array FixedBools : 3 * bool
dyn_array Bools : u8 * bool

pack Packed
- enabled : bool
- flags : FixedBools

seq Message
- enabled : bool
- packed : Packed
- values : Bools
"#,
    );

    let rust_out = temp_path("rust_bool", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(rust_src.contains("pub struct WireBool(u8);"));
    assert!(rust_src.contains("pub enabled: WireBool,"));
    assert!(rust_src.contains("pub struct FixedBools(pub [WireBool; 3]);"));
    assert!(rust_src.contains("pub enabled: bool,"));
    assert!(rust_src.contains("match u8::jaw_read(reader)?"));
    assert!(rust_src.contains("writer.write_all(bytemuck::bytes_of(self))"));
    assert!(!rust_src.contains("impl JawWrite for Packed\n{\n    fn jaw_write<W: Write>(&self, writer: &mut W) -> io::Result<()>\n    {\n        self.enabled.try_get"));
    let _ = fs::remove_file(&rust_out);

    let cpp_out = temp_path("cpp_bool", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(cpp_src.contains("class WireBool"));
    assert!(cpp_src.contains("std::optional<bool> value() const"));
    assert!(cpp_src.contains("WireBool enabled;"));
    assert!(cpp_src.contains("std::array<WireBool, 3>"));
    assert!(cpp_src.contains("std::vector<bool>"));
    assert!(cpp_src.contains("raw > 1"));
    assert!(cpp_src.contains("return write_scalar(writer, value);"));
    let _ = fs::remove_file(&cpp_out);

    let py_out = temp_path("python_bool", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(py_src.contains("class WireBool:"));
    assert!(py_src.contains("enabled: WireBool"));
    assert!(py_src.contains("enabled: bool"));
    assert!(py_src.contains("WireBool._from_raw"));
    assert!(py_src.contains("values.append(value.enabled._raw)"));
    assert!(py_src.contains("raise ValueError(f'invalid boolean byte: {value}')"));
    let _ = fs::remove_file(&py_out);
}

#[test]
fn logical_bool_does_not_pull_in_pod_wrapper() {
    let world = world_from("seq Message\n- enabled : bool\n");

    let rust_out = temp_path("rust_logical_bool", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(rust_src.contains("impl JawRead for bool"));
    assert!(!rust_src.contains("pub struct WireBool"));
    let _ = fs::remove_file(&rust_out);

    let cpp_out = temp_path("cpp_logical_bool", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(cpp_src.contains("raw > 1"));
    assert!(!cpp_src.contains("class WireBool"));
    let _ = fs::remove_file(&cpp_out);

    let py_out = temp_path("python_logical_bool", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(py_src.contains("def read_bool(self):"));
    assert!(!py_src.contains("class WireBool:"));
    let _ = fs::remove_file(&py_out);
}

#[test]
fn cpp_header_includes_helper_dependencies() {
    let world = world_from(
        r#"
dyn_array Bytes : u32 * u8
"#,
    );

    let cpp_out = temp_path("cpp_helper_dependencies", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");

    for header in ["algorithm", "cstring", "vector"] {
        assert!(
            cpp_src.contains(&format!("#include <{header}>")),
            "cpp output should directly include <{header}>"
        );
    }

    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn cpp_variant_reads_move_payloads() {
    let world = world_from(
        r#"
seq Payload
- value : u8

variant Message : u8
- 1 => Payload
"#,
    );

    let cpp_out = temp_path("cpp_variant_move", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(
        cpp_src.contains("std::move(payload)"),
        "cpp variant reads should move decoded payloads"
    );
    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn python_emits_cached_pod_codecs_and_buffer_writes() {
    let world = world_from(
        r#"
pack Inner
- a : u8
- b : u64

fixed_array Tail : 4 * u8

pack Outer
- inner : Inner
- tail : Tail
"#,
    );

    let py_out = temp_path("python_pod_codec", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(
        py_src.contains("_Outer_STRUCT = struct.Struct('<BQBBBB')"),
        "python should flatten nested POD layouts into one cached codec"
    );
    assert!(
        py_src.contains("values = iter(reader._read_struct(_Outer_STRUCT))"),
        "python pack reads should use the cached codec"
    );
    assert!(
        py_src.contains("writer._write_struct(_Outer_STRUCT, *values)"),
        "python pack writes should use the cached codec"
    );
    assert!(
        py_src.contains("writer.write_bytes(memoryview(arr).cast('B'))"),
        "python primitive arrays should avoid tobytes allocation"
    );
    assert!(
        py_src.contains("@dataclass(**_DATACLASS_SLOTS)"),
        "python data classes should use slots on supported interpreters"
    );
    assert!(
        !py_src.contains("struct.calcsize"),
        "python scalar reads should use cached Struct sizes"
    );
    let _ = fs::remove_file(&py_out);
}

#[test]
fn const_aliases_are_emitted_in_all_generators() {
    let world = world_from(
        r#"
const BYTE : u8 = 10
"#,
    );

    // Rust
    let rust_out = temp_path("rust_const_alias", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(
        rust_src.contains("pub const BYTE : u8 = 10;"),
        "rust should emit type alias for const"
    );
    let _ = fs::remove_file(&rust_out);

    // Python
    let py_out = temp_path("python_const_alias", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(
        py_src.contains("BYTE : int = 10"),
        "python should emit alias for const"
    );
    let _ = fs::remove_file(&py_out);

    // C++
    let cpp_out = temp_path("cpp_const_alias", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");

    println!("{cpp_src}");
    assert!(
        cpp_src.contains("constexpr inline std::uint8_t BYTE = 10;"),
        "cpp should emit alias for const"
    );
    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn empty_sequences_are_named_zero_wire_types_in_all_generators() {
    let world = world_from("seq Empty\n");

    let rust_out = temp_path("rust_empty_seq", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(rust_src.contains("pub struct Empty;"));
    assert!(rust_src.contains("pub struct EmptyView;"));
    assert!(rust_src.contains("Ok(Empty)"));
    assert!(rust_src.contains("impl JawWrite for Empty"));
    let _ = fs::remove_file(&rust_out);

    let python_out = temp_path("python_empty_seq", "py");
    codegen::emit_python(&world, &Default::default(), &python_out).expect("emit python");
    let python_src = fs::read_to_string(&python_out).expect("read python output");
    assert!(python_src.contains("class Empty:\n    pass"));
    assert!(python_src.contains("def read_Empty(reader):\n    return Empty("));
    assert!(python_src.contains("def write_Empty(writer, value):\n    return"));
    let _ = fs::remove_file(&python_out);

    let cpp_out = temp_path("cpp_empty_seq", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(cpp_src.contains("struct EmptyReader"));
    assert!(cpp_src.contains("struct EmptyWriter"));
    assert!(cpp_src.contains("read(Reader& reader, EmptyReader& value)"));
    assert!(cpp_src.contains("write(Writer& writer, EmptyWriter const& value)"));
    assert_eq!(cpp_src.matches("return true;").count(), 2);
    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn rust_bitfield_setters_widen_member_values_before_masking() {
    let world = world_from(
        r#"
enum Small : u8
- Value = 1

bits Wide : u32
- 0-7 primitive : u8
- 8-15 enumeration : Small
"#,
    );

    let rust_out = temp_path("rust_bitfield_widen", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");

    assert!(rust_src.contains("let v = v as u32;"));
    assert!(rust_src.contains("let v = v.raw() as u32;"));

    let _ = fs::remove_file(&rust_out);
}

#[test]
fn cpp_dynamic_arrays_are_nominal_types() {
    let world = world_from(
        r#"
dyn_array AnyString : u32 * u8
dyn_array AnyBytes : u32 * u8

variant Any : u8
- 1 => AnyString
- 2 => AnyBytes
"#,
    );

    let cpp_out = temp_path("cpp_nominal_arrays", "hpp");
    codegen::emit_cpp(&world, &Default::default(), &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");

    assert!(cpp_src.contains("struct AnyStringReader : ArrayRef<std::uint8_t>"));
    assert!(cpp_src.contains("struct AnyBytesReader : ArrayRef<std::uint8_t>"));
    assert!(cpp_src.contains("struct AnyStringWriter : std::span<std::uint8_t>"));
    assert!(cpp_src.contains("struct AnyBytesWriter : std::span<std::uint8_t>"));
    assert!(cpp_src.contains("AnyStringWriter(Base value) : Base(std::move(value)) {}"));
    assert!(cpp_src.contains("struct AnyReader : std::variant<AnyStringReader, AnyBytesReader>"));
    assert!(!cpp_src.contains("using AnyStringReader ="));

    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn python_escapes_keyword_identifiers() {
    let world = world_from(
        r#"
enum Optional : u8
- None = 0
- Value = 1

seq Message
- class : Optional
"#,
    );

    let python_out = temp_path("python_keywords", "py");
    codegen::emit_python(&world, &Default::default(), &python_out).expect("emit python");
    let python_src = fs::read_to_string(&python_out).expect("read python output");

    assert!(python_src.contains("None_ = 0"));
    assert!(python_src.contains("Optional.None_ = Optional(0)"));
    assert!(python_src.contains("class_: Optional"));
    assert!(!python_src.contains("\n    None = 0"));

    let _ = fs::remove_file(&python_out);
}
