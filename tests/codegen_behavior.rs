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
