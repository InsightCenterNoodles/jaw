use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Smoke tests over emitted source to ensure critical guards/naming make it into generated code.
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
fn dynamic_array_write_checks_length_limits() {
    let world = world_from(
        r#"
dyn_array Arr : u8 * u8
"#,
    );

    // Rust
    let rust_out = temp_path("rust_dyn_guard", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(
        rust_src.contains("array length too large to encode"),
        "rust output missing length guard"
    );
    assert!(
        rust_src.contains("max_len"),
        "rust output missing max_len guard"
    );
    let _ = fs::remove_file(&rust_out);

    // Python
    let py_out = temp_path("python_dyn_guard", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(
        py_src.contains("array length too large to encode"),
        "python output missing length guard"
    );
    assert!(
        py_src.contains("len(values) >"),
        "python output missing len guard"
    );
    let _ = fs::remove_file(&py_out);
}

#[test]
fn variant_case_names_are_unique() {
    let world = world_from(
        r#"
pack Foo
- a : u8

variant V : u8
- 1 => Foo
- 2 => Foo
"#,
    );

    // Rust
    let rust_out = temp_path("rust_variant_names", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(
        rust_src.contains("Foo_1"),
        "rust output missing first case name"
    );
    assert!(
        rust_src.contains("Foo_2"),
        "rust output missing second case name"
    );
    let _ = fs::remove_file(&rust_out);

    // Python
    let py_out = temp_path("python_variant_names", "py");
    codegen::emit_python(&world, &Default::default(), &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(
        py_src.contains("make_Foo_1"),
        "python output missing first case helper"
    );
    assert!(
        py_src.contains("type_Foo_2"),
        "python output missing second case constant"
    );
    let _ = fs::remove_file(&py_out);
}

#[test]
fn optional_guard_emits_valid_code() {
    let world = world_from(
        r#"
dyn_array Arr : u32 * u16

pack MyPOD
- v : u8

dyn_array ObjArr : u32 * MyPOD
"#,
    );
    let opts = GlobalOptions {
        guard_array_size: Some(16),
    };

    // Rust: should not emit C++ tokens when guard enabled
    let rust_out = temp_path("rust_guard_enabled", "rs");
    codegen::emit_rust(&world, &opts, &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");
    assert!(rust_src.contains("array too large"));
    assert!(!rust_src.contains("static_cast"));
    let _ = fs::remove_file(&rust_out);

    // Python: should raise a real exception type
    let py_out = temp_path("python_guard_enabled", "py");
    codegen::emit_python(&world, &opts, &py_out).expect("emit python");
    let py_src = fs::read_to_string(&py_out).expect("read python output");
    assert!(py_src.contains("raise ValueError('array too large')"));
    assert!(!py_src.contains("raise \""));
    let _ = fs::remove_file(&py_out);

    // C++: should use division to avoid overflow
    let cpp_out = temp_path("cpp_guard_enabled", "hpp");
    codegen::emit_cpp(&world, &opts, &cpp_out).expect("emit cpp");
    let cpp_src = fs::read_to_string(&cpp_out).expect("read cpp output");
    assert!(cpp_src.contains("const uint64_t byte_limit = 16ULL;"));
    let _ = fs::remove_file(&cpp_out);
}

#[test]
fn rust_write_variant_payload_reference_has_declared_lifetime() {
    let world = world_from(
        r#"
pack P
- a : u8

variant V : u8
- 1 => P
"#,
    );

    let rust_out = temp_path("rust_variant_payload_lt", "rs");
    codegen::emit_rust(&world, &Default::default(), &rust_out).expect("emit rust");
    let rust_src = fs::read_to_string(&rust_out).expect("read rust output");

    assert!(rust_src.contains("pub enum VView<'a>"));
    assert!(
        rust_src.contains("P_1(&'a P),"),
        "write variant payload reference should use the enum lifetime"
    );

    let _ = fs::remove_file(&rust_out);
}
