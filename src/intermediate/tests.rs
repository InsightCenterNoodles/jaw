use std::collections::HashMap;
use std::path::PathBuf;

use super::*;

/// Summarizes struct members as `(name, type)` for assertions.
fn struct_sig(members: &[StructMember]) -> Vec<(&str, &str)> {
    members
        .iter()
        .map(|m| (m.name.as_str(), m.ty.as_str()))
        .collect()
}

/// Summarizes enum members as `(name, value)` for assertions.
fn enum_sig(members: &[EnumMember]) -> Vec<(&str, i64)> {
    members.iter().map(|m| (m.name.as_str(), m.value)).collect()
}

/// Summarizes bitfield members as `(range, name, type)` for assertions.
fn bit_sig(members: &[BitfldMember]) -> Vec<(&str, &str, &str)> {
    members
        .iter()
        .map(|m| (m.range.as_str(), m.name.as_str(), m.ty.as_str()))
        .collect()
}

/// Summarizes variant members as `(discriminant, payload_type)` for assertions.
fn variant_sig(members: &[VariantMember]) -> Vec<(u64, &str)> {
    members.iter().map(|m| (m.value, m.ty.as_str())).collect()
}

fn parse_err(source: &str) -> IntermediateError {
    Module::from_string("file".into(), source.into()).expect_err("parse should fail")
}

fn assert_missing_detail(source: &str, kind: &str) {
    let err = parse_err(source);
    let IntermediateError::MissingDeclarationDetail { kind: actual, .. } = err else {
        panic!("unexpected error kind: {err:?}");
    };
    assert_eq!(actual, kind);
}

fn assert_malformed_member(source: &str, expected: &str) {
    let err = parse_err(source);
    let IntermediateError::MalformedMember { reason, .. } = err else {
        panic!("unexpected error kind: {err:?}");
    };
    assert!(
        reason.contains(expected),
        "expected `{expected}` in error reason `{reason}`"
    );
}

/// Test: the example DSL file parses into the expected intermediate AST.
#[test]
fn intermediate() {
    let source = include_str!("../../assets/example.jaw");

    let module = Module::from_string("file".into(), source.into()).expect("parse module");

    let m: HashMap<_, _> = module
        .definitions
        .into_iter()
        .map(|x| (x.ident.clone(), x))
        .collect();

    assert_eq!(m.len(), 15);

    let quick_typename = |name: &str| -> TypeName {
        TypeName::from_string(
            SourceLocation::new(
                SourceCode(std::sync::Arc::new(module.source.clone())),
                Position { line: 0, column: 0 },
            ),
            name,
        )
        .unwrap()
    };

    match &m[&quick_typename("MyPOD")].kind {
        TypeKind::Pack(pack) => {
            assert_eq!(
                struct_sig(&pack.members),
                vec![("a_thing", "u8"), ("b_thing", "u64")]
            );
        }
        other => panic!("MyPOD parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("MyOtherPOD")].kind {
        TypeKind::Pack(pack) => {
            assert_eq!(
                struct_sig(&pack.members),
                vec![("first", "MyPOD"), ("second", "FixedString")]
            );
        }
        other => panic!("MyOtherPOD parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("PlainEnum")].kind {
        TypeKind::Enum(e) => {
            assert_eq!(e.ty.as_str(), "u8");
            assert!(e.reserved.is_empty());
            assert_eq!(enum_sig(&e.members), vec![("F1", 0), ("F2", 1)]);
        }
        other => panic!("PlainEnum parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("BetterEnum")].kind {
        TypeKind::Enum(e) => {
            assert_eq!(e.ty.as_str(), "u8");
            assert_eq!(enum_sig(&e.members), vec![("A", 0), ("B", 1)]);
            assert_eq!(
                e.reserved
                    .iter()
                    .map(|r| (r.start, r.end))
                    .collect::<Vec<_>>(),
                vec![(2, 255)]
            );
        }
        other => panic!("BetterEnum parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("MyFlags")].kind {
        TypeKind::Bitfld(bits) => {
            assert_eq!(bits.ty.as_str(), "u8");
            assert_eq!(
                bit_sig(&bits.members),
                vec![
                    ("0", "is_thing", "u8"),
                    ("1-2", "another_thing", "u8"),
                    ("3-4", "some_stuff", "PlainEnum")
                ]
            );
        }
        other => panic!("MyFlags parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("SmallSeq")].kind {
        TypeKind::Sequence(seq) => {
            assert_eq!(struct_sig(&seq.members), vec![("list", "Data")]);
        }
        other => panic!("SmallSeq parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("MyPODFixedList")].kind {
        TypeKind::FixedArray(arr) => {
            assert_eq!(arr.count, 8);
            assert_eq!(arr.value_type.as_str(), "MyPOD");
        }
        other => panic!("MyPODFixedList parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("MyOtherPODDynList")].kind {
        TypeKind::DynamicArray(arr) => {
            assert_eq!(arr.size_type.as_str(), "u16");
            assert_eq!(arr.value_type.as_str(), "MyOtherPOD");
        }
        other => panic!("MyOtherPODDynList parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("ComplexSeq")].kind {
        TypeKind::Sequence(seq) => {
            assert_eq!(
                struct_sig(&seq.members),
                vec![
                    ("flags", "MyFlags"),
                    ("list", "MyPODFixedList"),
                    ("other_list", "MyOtherPODDynList")
                ]
            );
        }
        other => panic!("ComplexSeq parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("Root")].kind {
        TypeKind::Sequence(seq) => {
            assert_eq!(
                struct_sig(&seq.members),
                vec![("name", "ShortString"), ("var", "MyVariant")]
            );
        }
        other => panic!("Root parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("FixedString")].kind {
        TypeKind::FixedArray(arr) => {
            assert_eq!(arr.count, 4);
            assert_eq!(arr.value_type.as_str(), "u8");
        }
        other => panic!("FixedString parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("ShortString")].kind {
        TypeKind::DynamicArray(arr) => {
            assert_eq!(arr.size_type.as_str(), "u8");
            assert_eq!(arr.value_type.as_str(), "u8");
        }
        other => panic!("ShortString parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("Data")].kind {
        TypeKind::DynamicArray(arr) => {
            assert_eq!(arr.size_type.as_str(), "u8");
            assert_eq!(arr.value_type.as_str(), "f32");
        }
        other => panic!("Data parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("MyVariant")].kind {
        TypeKind::Variant(var) => {
            assert_eq!(var.ty.as_str(), "u8");
            assert_eq!(
                variant_sig(&var.members),
                vec![
                    (1, "MyPOD"),
                    (2, "MyOtherPOD"),
                    (3, "void"),
                    (4, "SmallSeq"),
                    (5, "ComplexSeq")
                ]
            );
        }
        other => panic!("MyVariant parsed as unexpected kind: {:?}", other),
    }
}

/// Test: `variant` declarations do not support default members.
#[test]
fn variant_default_is_rejected() {
    let source = r#"
variant Bad : u8
> 0 => void
"#;

    let err = Module::from_string("file".into(), source.into()).expect_err("parse should fail");

    let IntermediateError::MalformedMember { reason, .. } = err else {
        panic!("unexpected error kind: {err:?}");
    };
    assert!(
        reason.contains("default members"),
        "unexpected error: {reason}"
    );
}

/// Test: import lines are parsed before declarations.
#[test]
fn imports_are_parsed() {
    let source = r#"
from "other.jaw" use {Thing, OtherThing}
pack Local
- a : u8
"#;

    let module = Module::from_string("file".into(), source.into()).unwrap();
    assert_eq!(module.imports.len(), 1);
    let imp = &module.imports[0];
    assert_eq!(imp.path, "other.jaw");
    assert!(!imp.import_all);
    assert_eq!(
        imp.types.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
        vec!["Thing", "OtherThing"]
    );
}

/// Test: wildcard imports are parsed.
#[test]
fn wildcard_imports_are_parsed() {
    let source = r#"
from "other.jaw" use *
pack Local
- a : u8
"#;

    let module = Module::from_string("file".into(), source.into()).unwrap();
    assert_eq!(module.imports.len(), 1);
    let imp = &module.imports[0];
    assert_eq!(imp.path, "other.jaw");
    assert!(imp.import_all);
    assert!(imp.types.is_empty());
}

/// Test: imports must appear before declarations.
#[test]
fn imports_after_declarations_are_rejected() {
    let source = r#"
pack Local
- a : u8
from "other.jaw" use {Thing}
"#;

    let err = Module::from_string("file".into(), source.into()).expect_err("parse should fail");
    let IntermediateError::ImportAfterDeclaration { .. } = err else {
        panic!("unexpected error kind: {err:?}");
    };
}

/// Test: `const` declarations parse with a single target type.
#[test]
fn const_declaration_is_parsed() {
    let source = r#"
const Byte : u8 = 1
"#;

    let module = Module::from_string("file".into(), source.into()).expect("parse module");
    assert_eq!(module.definitions.len(), 1);

    match &module.definitions[0].kind {
        TypeKind::Const(c) => {
            assert_eq!(module.definitions[0].ident.as_str(), "Byte");
            assert_eq!(c.ty.as_str(), "u8");
            assert_eq!(c.value, "1");
        }
        other => panic!("Byte parsed as unexpected kind: {:?}", other),
    }
}

/// Test: comments, blank lines, and extra whitespace are ignored by the parser.
#[test]
fn comments_and_whitespace_are_ignored() {
    let source = r#"
# file header

  pack   Local    # inline declaration comment
  -   a   :   u8  # inline member comment

seq Root
- value : Local
"#;

    let module = Module::from_string("file".into(), source.into()).expect("parse module");
    assert_eq!(module.definitions.len(), 2);

    match &module.definitions[0].kind {
        TypeKind::Pack(pack) => assert_eq!(struct_sig(&pack.members), vec![("a", "u8")]),
        other => panic!("Local parsed as unexpected kind: {:?}", other),
    }
    match &module.definitions[1].kind {
        TypeKind::Sequence(seq) => assert_eq!(struct_sig(&seq.members), vec![("value", "Local")]),
        other => panic!("Root parsed as unexpected kind: {:?}", other),
    }
}

/// Test: enum default members are no longer accepted.
#[test]
fn enum_default_is_rejected() {
    let source = r#"
enum Mode : u8
> Unknown = 255
- Ready = 1
"#;

    assert_malformed_member(source, "default members are no longer supported");
}

#[test]
fn enum_and_variant_reservations_are_parsed() {
    let source = r#"
enum Mode : u8
- READY = 1
- reserve 2
- reserve 20..=23

variant Message : u8
- 1 => void
- reserve 7..=9
"#;
    let module = Module::from_string("file".into(), source.into()).expect("parse module");

    let TypeKind::Enum(enm) = &module.definitions[0].kind else {
        panic!("expected enum");
    };
    assert_eq!(
        enm.reserved
            .iter()
            .map(|r| (r.start, r.end))
            .collect::<Vec<_>>(),
        vec![(2, 2), (20, 23)]
    );

    let TypeKind::Variant(var) = &module.definitions[1].kind else {
        panic!("expected variant");
    };
    assert_eq!(
        var.reserved
            .iter()
            .map(|r| (r.start, r.end))
            .collect::<Vec<_>>(),
        vec![(7, 9)]
    );
}

/// Test: declarations that require a `: ...` detail reject missing detail.
#[test]
fn declaration_details_are_required() {
    for (source, kind) in [
        ("enum Missing\n- A = 1\n", "enum"),
        ("bits Missing\n- 0 a : u8\n", "bits"),
        ("variant Missing\n- 0 => void\n", "variant"),
        ("fixed_array Missing\n", "fixed_array"),
        ("dyn_array Missing\n", "dyn_array"),
        ("const Missing\n", "const"),
        ("const Missing : u8\n", "missing value"),
    ] {
        assert_missing_detail(source, kind);
    }
}

/// Test: all array declarations require a count/size and element type separated by `*`.
#[test]
fn malformed_array_specs_are_rejected() {
    for source in ["fixed_array Bad : 4 u8\n", "dyn_array Bad : u8 u8\n"] {
        let err = parse_err(source);
        let IntermediateError::InvalidArraySpec { .. } = err else {
            panic!("unexpected error kind: {err:?}");
        };
    }
}

/// Test: unsupported top-level declaration keywords are rejected.
#[test]
fn unsupported_declarations_are_rejected() {
    let err = parse_err("struct Bad\n- a : u8\n");
    let IntermediateError::UnsupportedDeclaration { decl_type, .. } = err else {
        panic!("unexpected error kind: {err:?}");
    };
    assert_eq!(decl_type, "struct");
}

/// Test: malformed member lines report the specific token-level failure.
#[test]
fn malformed_members_are_rejected() {
    assert_malformed_member("pack Bad\n- a u8\n", "expected `:`");
    assert_malformed_member("pack Bad\n- a : u8 extra\n", "unexpected extra token");
    assert_malformed_member("enum Bad : u8\n- A : 1\n", "expected `=`");
    assert_malformed_member("bits Bad : u8\n- 0 flag u8\n", "expected `:`");
    assert_malformed_member("variant Bad : u8\n- 0 -> void\n", "expected `=>`");
}

/// Test: integer-valued members reject non-integer text.
#[test]
fn member_numbers_must_parse() {
    for source in [
        "enum Bad : u8\n- A = nope\n",
        "variant Bad : u8\n- nope => void\n",
        "fixed_array Bad : nope * u8\n",
    ] {
        let err = parse_err(source);
        let IntermediateError::InvalidNumber { .. } = err else {
            panic!("unexpected error kind: {err:?}");
        };
    }
}

/// Test: no declaration supports default members.
#[test]
fn default_member_rules_are_enforced() {
    assert_malformed_member(
        "pack Bad\n> a : u8\n",
        "pack declarations do not support default members",
    );
    assert_malformed_member(
        "seq Bad\n> a : u8\n",
        "sequence declarations do not support default members",
    );
    assert_malformed_member(
        "bits Bad : u8\n> 0 a : u8\n",
        "bitfield declarations do not support default members",
    );

    assert_malformed_member(
        "enum Bad : u8\n> A = 0\n",
        "default members are no longer supported",
    );
}

/// Test: type names may contain only alphanumeric characters and underscores.
#[test]
fn illegal_type_name_characters_are_rejected() {
    for source in [
        "pack Bad-Type\n- a : u8\n",
        "pack Bad\n- a : bad-type\n",
        "dyn_array Bad : u8 * bad-type\n",
        "from \"other.jaw\" use {Bad-Type}\npack Local\n- a : u8\n",
    ] {
        let err = parse_err(source);
        let IntermediateError::IllegalChar { reason: '-', .. } = err else {
            panic!("unexpected error kind: {err:?}");
        };
    }
}

/// Test: import declarations validate their full shape.
#[test]
fn malformed_imports_are_rejected() {
    for (source, expected) in [
        (
            "from other.jaw use {Thing}\npack Local\n- a : u8\n",
            "double-quotation",
        ),
        (
            "from \"other.jaw\" {Thing}\npack Local\n- a : u8\n",
            "expected `use`",
        ),
        (
            "from \"other.jaw\" use Thing\npack Local\n- a : u8\n",
            "expected `{...}`",
        ),
        (
            "from \"other.jaw\" use {}\npack Local\n- a : u8\n",
            "may not be empty",
        ),
    ] {
        let err = parse_err(source);
        let IntermediateError::InvalidImport { reason, .. } = err else {
            panic!("unexpected error kind: {err:?}");
        };
        assert!(
            reason.contains(expected),
            "expected `{expected}` in error reason `{reason}`"
        );
    }
}

fn unique_test_dir(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("jaw_{name}_{}_{}", std::process::id(), stamp))
}

/// Test: wildcard imports pull all top-level definitions from the imported module.
#[test]
fn wildcard_imports_load_all_types() {
    let dir = unique_test_dir("wildcard_imports");
    std::fs::create_dir_all(&dir).unwrap();

    let common_path = dir.join("common.jaw");
    let root_path = dir.join("root.jaw");

    std::fs::write(
        &common_path,
        r#"
pack CommonA
- field : u8

seq CommonB
- value : CommonA
"#,
    )
    .unwrap();

    std::fs::write(
        &root_path,
        r#"
from "common.jaw" use *

seq Local
- first : CommonA
- second : CommonB
"#,
    )
    .unwrap();

    let module = load_module_with_imports(&root_path).unwrap();
    let names: Vec<&str> = module
        .definitions
        .iter()
        .map(|d| d.ident.as_str())
        .collect();
    assert!(names.contains(&"Local"));
    assert!(names.contains(&"CommonA"));
    assert!(names.contains(&"CommonB"));

    std::fs::remove_dir_all(&dir).unwrap();
}
