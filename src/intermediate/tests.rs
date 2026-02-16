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
            assert!(e.default.is_none());
            assert_eq!(enum_sig(&e.members), vec![("F1", 0), ("F2", 1)]);
        }
        other => panic!("PlainEnum parsed as unexpected kind: {:?}", other),
    }

    match &m[&quick_typename("BetterEnum")].kind {
        TypeKind::Enum(e) => {
            assert_eq!(e.ty.as_str(), "u8");
            assert_eq!(
                e.default.as_ref().map(|d| (d.name.as_str(), d.value)),
                Some(("DEFAULT", 255))
            );
            assert_eq!(enum_sig(&e.members), vec![("A", 0), ("B", 1)]);
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
        reason.contains("do not support default"),
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
        }
        other => panic!("Byte parsed as unexpected kind: {:?}", other),
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
