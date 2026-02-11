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
alias A : B
alias B : A
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject cycles");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("cyclic"),
        "unexpected error message: {msg}"
    );
}

/// Test: direct self-references are rejected.
#[test]
fn self_references_are_rejected() {
    let src = r#"
alias A : A
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject self reference");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("cyclic"),
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

/// Test: unknown type references are rejected.
#[test]
fn unknown_types_are_rejected() {
    let src = r#"
alias A : Missing
"#;

    let module = intermediate::Module::from_string("file".into(), src.into()).unwrap();
    let err = compile(module).expect_err("compile should reject unknown type names");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Missing"),
        "unexpected error message, wanted type name: {msg}"
    );
}
