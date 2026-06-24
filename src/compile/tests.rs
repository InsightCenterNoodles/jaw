use super::*;
use crate::intermediate;

fn compile_src(src: &str) -> anyhow::Result<World> {
    let module = intermediate::Module::from_string("file".into(), src.into())?;
    compile(module)
}

fn compile_err(src: &str) -> String {
    let err = compile_src(src).expect_err("compile should fail");
    format!("{err:#}")
}

fn assert_compile_err_contains(src: &str, expected: &str) {
    let msg = compile_err(src);
    assert!(
        msg.contains(expected),
        "expected `{expected}` in error message:\n{msg}"
    );
}

/// Test: overlapping bitfield ranges are rejected.
#[test]
fn bitfield_overlap_is_rejected() {
    let src = r#"
bits Bad : u8
- 0-2 a : u8
- 2-3 b : u8
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject overlapping bit ranges");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("overlaps") || msg.contains("overlap"),
        "unexpected error message: {msg}"
    );
}

/// Test: dynamic arrays cannot have `void` element types.
#[test]
fn dynarray_void_element_is_rejected() {
    let src = r#"
dyn_array Bad : u8 * void
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject void element type");
    let msg = format!("{err:#}");
    assert!(msg.contains("void"), "unexpected error message: {msg}");
}

/// Test: cyclic type references are rejected.
#[test]
fn cycles_are_rejected() {
    let src = r#"
pack A
- thing : B
pack B
- other_thing : A
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject cycles");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("cyclic"),
        "unexpected error message: {msg}"
    );
}

/// Test: recursive types through dynamic arrays are allowed.
#[test]
fn dynarray_cycles_are_allowed() {
    let src = r#"
dyn_array AnyBytes : u32 * u8
dyn_array AnyString : u32 * u8
dyn_array AnyArray : u32 * Any
dyn_array AnyMap : u32 * AnyKeyPair

seq AnyKeyPair
- key : Any
- value : Any

variant Any : u8
- 0 => void
- 1 => u8
- 2 => i64
- 3 => f64
- 4 => AnyString
- 5 => AnyArray
- 6 => AnyMap
- 7 => AnyBytes
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    compile(module).expect("compile should accept dynarray-based recursion");
}

/// Test: const declarations must target primitive types for now.
#[test]
fn const_target_must_be_primitive() {
    let src = r#"
pack P
- a : u8

const Bad : P = 10
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject non-primitive const target");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not primitive"),
        "unexpected error message: {msg}"
    );
}

/// Test: topological sorting is deterministic and stable.
#[test]
fn topological_sort_is_stable() {
    let src = r#"
pack A
- a : u8

pack B
- b : u8

pack C
- a : A
- b : B
"#;

    let expected = vec!["A", "B", "C"];
    let mut seen_orders = vec![];

    for _ in 0..8 {
        let module = intermediate::Module::from_string("file".into(), src.to_string()).unwrap();
        let world = compile(module).expect("compile should succeed");
        let names: Vec<String> = world
            .iter()
            .filter(|(_, ty)| !matches!(ty.kind, TypeKind::Void | TypeKind::Primitive(_)))
            .map(|(_, ty)| ty.ident.to_string())
            .collect();

        assert_eq!(names, expected, "toposort should respect declaration order");
        seen_orders.push(names);
    }

    let first = seen_orders.first().expect("at least one order recorded");
    for order in seen_orders.iter().skip(1) {
        assert_eq!(
            order, first,
            "toposort should be deterministic across invocations"
        );
    }
}

/// Test: duplicate user-defined names and builtin redefinitions are rejected.
#[test]
fn duplicate_type_names_are_rejected() {
    assert_compile_err_contains(
        r#"
pack Duplicate
- a : u8

seq Duplicate
- b : u8
"#,
        "duplicate type name Duplicate",
    );

    assert_compile_err_contains(
        r#"
pack u8
- a : u8
"#,
        "duplicate type name u8",
    );
}

/// Test: every declaration form reports unresolved type references.
#[test]
fn unknown_type_references_are_rejected() {
    for src in [
        "pack Bad\n- a : Missing\n",
        "seq Bad\n- a : Missing\n",
        "enum Bad : Missing\n- A = 1\n",
        "bits Bad : u8\n- 0 a : Missing\n",
        "variant Bad : u8\n- 0 => Missing\n",
        "dyn_array Bad : u8 * Missing\n",
        "fixed_array Bad : 1 * Missing\n",
        "const Bad : Missing = 1\n",
    ] {
        assert_compile_err_contains(src, "unknown type name `Missing`");
    }
}

/// Test: pack/fixed-array POD restrictions are enforced, while sequences can hold dynamic values.
#[test]
fn pod_rules_are_enforced() {
    compile_src(
        r#"
dyn_array Bytes : u8 * u8
seq Root
- bytes : Bytes
"#,
    )
    .expect("sequences may contain dynamic arrays");

    assert_compile_err_contains(
        r#"
dyn_array Bytes : u8 * u8
pack Bad
- bytes : Bytes
"#,
        "not POD",
    );

    assert_compile_err_contains(
        r#"
seq Payload
- value : u8

fixed_array Bad : 2 * Payload
"#,
        "not POD",
    );

    assert_compile_err_contains(
        r#"
pack Bad
- nothing : void
"#,
        "not POD",
    );
}

/// Test: struct-like declarations require unique member names.
#[test]
fn struct_member_names_must_be_unique() {
    for src in [
        "pack Bad\n- a : u8\n- a : u16\n",
        "seq Bad\n- a : u8\n- a : u16\n",
    ] {
        assert_compile_err_contains(src, "all names must be unique: a");
    }
}

/// Test: enum declarations require an unsigned integer base, unique names/values, and fitting values.
#[test]
fn enum_rules_are_enforced() {
    assert_compile_err_contains("enum Bad : i8\n- A = 1\n", "not integer with sign Unsigned");
    assert_compile_err_contains(
        "enum Bad : u8\n- A = 1\n- A = 2\n",
        "all names must be unique: A",
    );
    assert_compile_err_contains(
        "enum Bad : u8\n- A = 1\n- B = 1\n",
        "Duplicate enumerant value",
    );
    assert_compile_err_contains("enum Bad : u8\n- A = 256\n", "cannot fit");
    assert_compile_err_contains("enum Bad : u8\n> Unknown = 256\n- A = 1\n", "cannot fit");
    assert_compile_err_contains(
        "enum Bad : u8\n> A = 0\n- A = 1\n",
        "all names must be unique: A",
    );
}

/// Test: bitfield declarations validate base/member types and ranges.
#[test]
fn bitfield_rules_are_enforced() {
    compile_src(
        r#"
enum Base : u8
- A = 0

bits Flags : Base
- 0 mode : Base
"#,
    )
    .expect("enum-backed bitfields and enum members should compile");

    assert_compile_err_contains(
        "bits Bad : i8\n- 0 a : u8\n",
        "not integer with sign Unsigned",
    );
    assert_compile_err_contains(
        "bits Bad : u8\n- 0 a : f32\n",
        "primitive integers or enums",
    );
    assert_compile_err_contains(
        "bits Bad : u8\n- 3-1 a : u8\n",
        "start 3 greater than end 1",
    );
    assert_compile_err_contains("bits Bad : u8\n- 8 a : u8\n", "exceeds underlying width 8");
    assert_compile_err_contains(
        "bits Bad : u8\n- 0 a : u8\n- 1 a : u8\n",
        "all names must be unique: a",
    );
    assert_compile_err_contains("bits Bad : u8\n- nope a : u8\n", "while parsing bit range");
}

/// Test: variant declarations validate discriminant type, uniqueness, and value bounds.
#[test]
fn variant_rules_are_enforced() {
    compile_src("variant Maybe : u8\n- 0 => void\n- 1 => u8\n").expect("void payloads are valid");

    assert_compile_err_contains(
        "variant Bad : i8\n- 0 => void\n",
        "not integer with sign Unsigned",
    );
    assert_compile_err_contains(
        "variant Bad : u8\n- 1 => void\n- 1 => u8\n",
        "duplicate discriminant",
    );
    assert_compile_err_contains("variant Bad : u8\n- 256 => void\n", "cannot fit");
}

/// Test: arrays validate length/count and element constraints.
#[test]
fn array_rules_are_enforced() {
    compile_src("fixed_array Bytes : 1 * u8\n").expect("positive fixed arrays should compile");
    compile_src("dyn_array Bytes : u8 * u8\n")
        .expect("unsigned-sized dynamic arrays should compile");

    assert_compile_err_contains("fixed_array Bad : 0 * u8\n", "positive element count");
    assert_compile_err_contains("fixed_array Bad : 1 * void\n", "not POD");
    assert_compile_err_contains(
        "dyn_array Bad : i8 * u8\n",
        "not integer with sign Unsigned",
    );
}

/// Test: const declarations parse and validate integer/float primitive literals.
#[test]
fn const_rules_are_enforced() {
    let world = compile_src(
        r#"
const Limit : u8 = 255
const Scale : f32 = 1.5
"#,
    )
    .expect("primitive integer and float constants should compile");

    let const_values: Vec<_> = world
        .iter()
        .filter_map(|(_, ty)| match &ty.kind {
            TypeKind::Const(c) => Some((ty.ident.to_string(), c.value)),
            _ => None,
        })
        .collect();
    assert!(const_values.contains(&("Limit".into(), PrimitiveLiteral::Integer(255))));
    assert!(const_values.contains(&("Scale".into(), PrimitiveLiteral::Real(1.5))));

    assert_compile_err_contains("const Bad : u8 = 256\n", "cannot fit");
    assert_compile_err_contains("const Bad : u8 = 1.5\n", "not compatible");
    assert_compile_err_contains("const Bad : f32 = 1\n", "not compatible");
    assert_compile_err_contains("const Bad : u8 = nope\n", "unable to parse literal");
}

/// Test: merge worlds
#[test]
fn merge_world_basic() {
    let world_1 = World {
        definitions: [(
            TypeID(100),
            Type {
                ident: TypeName::from_string(Default::default(), "type_1").unwrap(),
                defined_at: Default::default(),
                kind: TypeKind::Primitive(Primitive {
                    width: BitWidth::W8,
                    sign: Signedness::Signed,
                    dtype: Datatype::Integer,
                }),
            },
        )]
        .into_iter()
        .collect(),
        sorted: vec![TypeID(100)],
        module_name: "test1".into(),
    };

    let world_2 = World {
        definitions: [(
            TypeID(200),
            Type {
                ident: TypeName::from_string(Default::default(), "type_2").unwrap(),
                defined_at: Default::default(),
                kind: TypeKind::Primitive(Primitive {
                    width: BitWidth::W8,
                    sign: Signedness::Signed,
                    dtype: Datatype::Integer,
                }),
            },
        )]
        .into_iter()
        .collect(),
        sorted: vec![TypeID(200)],
        module_name: "test2".into(),
    };

    let merge_truth = World {
        definitions: [
            (
                TypeID(100),
                Type {
                    ident: TypeName::from_string(Default::default(), "type_1").unwrap(),
                    defined_at: Default::default(),
                    kind: TypeKind::Primitive(Primitive {
                        width: BitWidth::W8,
                        sign: Signedness::Signed,
                        dtype: Datatype::Integer,
                    }),
                },
            ),
            (
                TypeID(300),
                Type {
                    ident: TypeName::from_string(Default::default(), "type_2").unwrap(),
                    defined_at: Default::default(),
                    kind: TypeKind::Primitive(Primitive {
                        width: BitWidth::W8,
                        sign: Signedness::Signed,
                        dtype: Datatype::Integer,
                    }),
                },
            ),
        ]
        .into_iter()
        .collect(),
        sorted: vec![TypeID(100), TypeID(300)],
        module_name: "test1".into(),
    };

    let merge = world_1.merge(world_2).expect("merge worlds");

    assert_eq!(merge.sorted, merge_truth.sorted);
    assert_eq!(merge.definitions, merge_truth.definitions);
    assert_eq!(merge.module_name, merge_truth.module_name);
}
