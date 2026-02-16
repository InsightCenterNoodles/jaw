use super::*;
use crate::intermediate;

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
