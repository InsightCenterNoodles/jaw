use std::{
    io::{BufRead, Read},
    iter::Peekable,
    ops::RangeInclusive,
    path::PathBuf,
};

#[derive(Debug, Clone, Copy)]
pub struct Position {
    line: usize,
    column: usize,
}

type TypeName = String;

#[derive(Debug)]
pub struct StructMember {
    pub name: TypeName,
    pub ty: String,
    pub defined_at: Position,
}

impl StructMember {
    fn parse(input: (usize, String)) -> Option<(MemberType, Self)> {
        let mut iter = make_mem_split(&input);
        let mem_ty = consume_member_start(&mut iter)?;

        let (place, name) = iter.next()?;

        dbg!(place, name);

        demand_string(&mut iter, ":");

        let ty = iter.next()?.1.to_string();

        demand_done(iter);

        Some((
            mem_ty,
            Self {
                name: name.into(),
                ty,
                defined_at: place,
            },
        ))
    }
}

/// Plain-old-data aggregate with C-like layout.
#[derive(Debug)]
pub struct Pack {
    pub members: Vec<StructMember>,
}

#[derive(Debug)]
pub struct EnumMember {
    pub name: String,
    pub value: i64,
    pub defined_at: Position,
}

impl EnumMember {
    fn parse(input: (usize, String)) -> Option<(MemberType, Self)> {
        let mut iter = make_mem_split(&input);
        let mem_ty = consume_member_start(&mut iter)?;

        dbg!(mem_ty);

        let (place, name) = iter.next()?;

        demand_string(&mut iter, "=");

        let value = iter.next()?.1.parse().unwrap();

        demand_done(iter);

        Some((
            mem_ty,
            Self {
                name: name.into(),
                value,
                defined_at: place,
            },
        ))
    }
}

/// Enum with an explicit primitive underlying type.
#[derive(Debug)]
pub struct Enum {
    pub ty: TypeName,
    pub members: Vec<EnumMember>,
    pub default: Option<EnumMember>,
}

#[derive(Debug)]
pub struct BitfldMember {
    pub name: String,
    pub ty: TypeName,
    pub range: String,
    pub defined_at: Position,
}

impl BitfldMember {
    fn parse(input: (usize, String)) -> Option<(MemberType, Self)> {
        let mut iter = make_mem_split(&input);
        let mem_ty = consume_member_start(&mut iter)?;

        let range = iter.next()?.1.to_string();

        let (place, name) = iter.next()?;

        demand_string(&mut iter, ":");

        let ty = iter.next()?.1.to_string();

        demand_done(iter);

        Some((
            mem_ty,
            Self {
                name: name.into(),
                ty,
                range,
                defined_at: place,
            },
        ))
    }
}

/// Bitfield backed by an integer/enum type with named bit ranges.
#[derive(Debug)]
pub struct Bitfld {
    pub ty: TypeName,
    pub members: Vec<BitfldMember>,
}

#[derive(Debug)]
pub struct VariantMember {
    pub ty: TypeName,
    pub value: u64,
    pub defined_at: Position,
}

impl VariantMember {
    fn parse(input: (usize, String)) -> Option<(MemberType, Self)> {
        let mut iter = make_mem_split(&input);
        let mem_ty = consume_member_start(&mut iter)?;

        let value = iter.next()?.1.parse().unwrap();

        demand_string(&mut iter, "=>");

        let (place, ty) = iter.next()?;

        demand_done(iter);

        Some((
            mem_ty,
            Self {
                ty: ty.into(),
                value,
                defined_at: place,
            },
        ))
    }
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub ty: TypeName,
    pub members: Vec<VariantMember>,
    pub default: Option<VariantMember>,
}

/// Sequence of named fields (a typical record/struct).
#[derive(Debug)]
pub struct Sequence {
    pub members: Vec<StructMember>,
}

#[derive(Debug)]
pub struct Alias {
    pub other: TypeName,
}

/// Array kinds
#[derive(Debug)]
pub struct DynamicArray {
    pub size_type: TypeName,
    pub value_type: TypeName,
}

#[derive(Debug)]
pub struct FixedArray {
    pub count: u64,
    pub value_type: TypeName,
}

#[derive(Debug)]
pub enum TypeKind {
    Alias(Alias),
    Pack(Pack),
    Enum(Enum),
    Bitfld(Bitfld),
    Variant(Variant),
    Sequence(Sequence),
    DynamicArray(DynamicArray),
    FixedArray(FixedArray),
}

#[derive(Debug)]
pub struct Type {
    pub ident: TypeName,
    pub defined_at: Position,
    pub kind: TypeKind,
}

pub struct Module {
    source: String,

    definitions: Vec<Type>,
}

impl Module {
    pub fn from_stream(source: String, stream: String) -> Option<Self> {
        let mut reader = Reader::new(stream);

        let mut module = Module {
            source,
            definitions: vec![],
        };

        while let Some(line) = reader.next_line() {
            dbg!(&line);

            // parse header
            let mut parts = line.1.split_whitespace();

            let Some(decl_type) = parts.next() else {
                continue;
            };

            let Some(decl_name) = parts.next() else {
                continue;
            };

            let extra = line.1.split_once(':').map(|x| x.1.trim());

            dbg!(extra);

            match decl_type {
                "pack" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_pack(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "seq" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_seq(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "enum" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_enum(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "bits" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_bitfld(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "variant" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_variant(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "fixed_array" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_fixed_array(extra)?,
                    ident: decl_name.to_owned(),
                }),
                "dyn_array" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_dyn_array(extra)?,
                    ident: decl_name.to_owned(),
                }),
                _ => return None,
            }
        }

        Some(module)
    }
}

struct Reader {
    source:
        Peekable<std::iter::Enumerate<std::io::Lines<std::io::BufReader<std::io::Cursor<String>>>>>,
}

impl Reader {
    fn new(s: String) -> Self {
        Self {
            source: std::io::BufReader::new(std::io::Cursor::new(s))
                .lines()
                .enumerate()
                .peekable(),
        }
    }

    fn next_line(&mut self) -> Option<(usize, String)> {
        loop {
            let x = self.source.next();

            match x {
                Some((num, Ok(ld))) => {
                    // sanitize
                    let mut string = ld.trim();

                    if let Some((a, _)) = string.split_once("#") {
                        string = a;
                    }

                    if string.is_empty() {
                        continue;
                    }

                    return Some((num, string.to_string()));
                }
                _ => return None,
            }
        }
    }

    fn member_iter<Func, U>(&mut self, mut f: Func) -> Option<(Vec<U>, Option<U>)>
    where
        Func: FnMut((usize, String)) -> Option<(MemberType, U)>,
    {
        let mut ret = vec![];
        let mut def = None;

        loop {
            let Some(l) = self.has_member_start() else {
                break;
            };

            let Some(item) = f(l) else {
                break;
            };

            match item.0 {
                MemberType::Normal => ret.push(item.1),
                MemberType::Default => {
                    assert!(def.is_none());
                    def = Some(item.1);
                }
            }
        }

        Some((ret, def))
    }

    fn has_member_start(&mut self) -> Option<(usize, String)> {
        let Some((_, Ok(line))) = self.source.peek() else {
            return None;
        };
        if line.starts_with("-") || line.starts_with(">") {
            self.next_line()
        } else {
            None
        }
    }

    fn parse_pack(&mut self, _extra: Option<&str>) -> Option<TypeKind> {
        let (members, default) = self.member_iter(StructMember::parse)?;

        dbg!(&members);

        assert!(default.is_none());

        Some(TypeKind::Pack(Pack { members }))
    }

    fn parse_seq(&mut self, _extra: Option<&str>) -> Option<TypeKind> {
        let (members, default) = self.member_iter(StructMember::parse)?;

        assert!(default.is_none());

        Some(TypeKind::Sequence(Sequence { members }))
    }

    fn parse_enum(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let ty = extra.unwrap().to_string();

        let (members, default) = self.member_iter(EnumMember::parse)?;

        Some(TypeKind::Enum(Enum {
            ty,
            members,
            default,
        }))
    }

    fn parse_bitfld(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let ty = extra.unwrap().to_string();

        let (members, default) = self.member_iter(BitfldMember::parse)?;

        assert!(default.is_none());

        Some(TypeKind::Bitfld(Bitfld { ty, members }))
    }

    fn parse_variant(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let ty = extra.unwrap().to_string();

        let (members, default) = self.member_iter(VariantMember::parse)?;

        Some(TypeKind::Variant(Variant {
            ty,
            members,
            default,
        }))
    }

    fn parse_fixed_array(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let parts = extra.unwrap().split_once('*').unwrap();

        dbg!(parts);

        Some(TypeKind::FixedArray(FixedArray {
            count: parts.0.trim().parse().unwrap(),
            value_type: parts.1.trim().into(),
        }))
    }

    fn parse_dyn_array(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let parts = extra.unwrap().split_once('*').unwrap();

        Some(TypeKind::DynamicArray(DynamicArray {
            size_type: parts.0.trim().into(),
            value_type: parts.1.trim().into(),
        }))
    }
}

#[derive(Debug, Clone, Copy)]
enum MemberType {
    Normal,
    Default,
}

fn addr_of(s: &str) -> usize {
    s.as_ptr() as usize
}

fn split_whitespace_indices(s: &str) -> impl Iterator<Item = (usize, &str)> {
    s.split_whitespace()
        .map(move |sub| (addr_of(sub) - addr_of(s), sub))
}

fn make_mem_split(input: &(usize, String)) -> impl Iterator<Item = (Position, &str)> {
    split_whitespace_indices(&input.1).map(|x| {
        (
            Position {
                line: input.0,
                column: x.0,
            },
            x.1,
        )
    })
}

fn consume_member_start<'a>(
    iter: &mut impl Iterator<Item = (Position, &'a str)>,
) -> Option<MemberType> {
    let n = iter.next()?;

    match n.1 {
        "-" => Some(MemberType::Normal),
        ">" => Some(MemberType::Default),
        _ => None,
    }
}

fn demand_string<'a>(iter: &mut impl Iterator<Item = (Position, &'a str)>, text: &str) {
    match iter.next() {
        Some((_, x)) if x == text => {
            return;
        }
        _ => panic!("NOPE"),
    }
}

fn demand_done<'a>(mut iter: impl Iterator<Item = (Position, &'a str)>) {
    assert!(matches!(iter.next(), None));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn struct_sig(members: &[StructMember]) -> Vec<(&str, &str)> {
        members
            .iter()
            .map(|m| (m.name.as_str(), m.ty.as_str()))
            .collect()
    }

    fn enum_sig(members: &[EnumMember]) -> Vec<(&str, i64)> {
        members.iter().map(|m| (m.name.as_str(), m.value)).collect()
    }

    fn bit_sig(members: &[BitfldMember]) -> Vec<(&str, &str, &str)> {
        members
            .iter()
            .map(|m| (m.range.as_str(), m.name.as_str(), m.ty.as_str()))
            .collect()
    }

    fn variant_sig(members: &[VariantMember]) -> Vec<(u64, &str)> {
        members.iter().map(|m| (m.value, m.ty.as_str())).collect()
    }

    #[test]
    fn intermediate() {
        let source = include_str!("../assets/basic.jaw");

        let m = Module::from_stream("file".into(), source.into()).unwrap();

        let m: HashMap<_, _> = m
            .definitions
            .into_iter()
            .map(|x| (x.ident.clone(), x))
            .collect();

        assert_eq!(m.len(), 11);

        match &m["MyPOD"].kind {
            TypeKind::Pack(pack) => {
                assert_eq!(
                    struct_sig(&pack.members),
                    vec![("a_thing", "u8"), ("b_thing", "u64")]
                );
            }
            other => panic!("MyPOD parsed as unexpected kind: {:?}", other),
        }

        match &m["MyOtherPOD"].kind {
            TypeKind::Pack(pack) => {
                assert_eq!(
                    struct_sig(&pack.members),
                    vec![("first", "MyPOD"), ("second", "FixedString")]
                );
            }
            other => panic!("MyOtherPOD parsed as unexpected kind: {:?}", other),
        }

        match &m["PlainEnum"].kind {
            TypeKind::Enum(e) => {
                assert_eq!(e.ty, "u8");
                assert!(e.default.is_none());
                assert_eq!(enum_sig(&e.members), vec![("F1", 0), ("F2", 1)]);
            }
            other => panic!("PlainEnum parsed as unexpected kind: {:?}", other),
        }

        match &m["BetterEnum"].kind {
            TypeKind::Enum(e) => {
                assert_eq!(e.ty, "u8");
                assert_eq!(
                    e.default.as_ref().map(|d| (d.name.as_str(), d.value)),
                    Some(("DEFAULT", 255))
                );
                assert_eq!(enum_sig(&e.members), vec![("A", 0), ("B", 1)]);
            }
            other => panic!("BetterEnum parsed as unexpected kind: {:?}", other),
        }

        match &m["MyFlags"].kind {
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

        match &m["SmallSeq"].kind {
            TypeKind::Sequence(seq) => {
                assert_eq!(struct_sig(&seq.members), vec![("list", "Data")]);
            }
            other => panic!("SmallSeq parsed as unexpected kind: {:?}", other),
        }

        match &m["Root"].kind {
            TypeKind::Sequence(seq) => {
                assert_eq!(
                    struct_sig(&seq.members),
                    vec![("name", "ShortString"), ("var", "MyVariant")]
                );
            }
            other => panic!("Root parsed as unexpected kind: {:?}", other),
        }

        match &m["FixedString"].kind {
            TypeKind::FixedArray(arr) => {
                assert_eq!(arr.count, 4);
                assert_eq!(arr.value_type.as_str(), "u8");
            }
            other => panic!("FixedString parsed as unexpected kind: {:?}", other),
        }

        match &m["ShortString"].kind {
            TypeKind::DynamicArray(arr) => {
                assert_eq!(arr.size_type.as_str(), "u8");
                assert_eq!(arr.value_type.as_str(), "u8");
            }
            other => panic!("ShortString parsed as unexpected kind: {:?}", other),
        }

        match &m["Data"].kind {
            TypeKind::DynamicArray(arr) => {
                assert_eq!(arr.size_type.as_str(), "u8");
                assert_eq!(arr.value_type.as_str(), "f32");
            }
            other => panic!("Data parsed as unexpected kind: {:?}", other),
        }

        match &m["MyVariant"].kind {
            TypeKind::Variant(var) => {
                assert_eq!(var.ty.as_str(), "u8");
                assert_eq!(
                    variant_sig(&var.members),
                    vec![(0, "MyPOD"), (1, "MyOtherPOD"), (2, "void")]
                );
                assert_eq!(
                    var.default
                        .as_ref()
                        .map(|d| (d.value, d.ty.as_str()))
                        .expect("variant default"),
                    (3, "SmallSeq")
                );
            }
            other => panic!("MyVariant parsed as unexpected kind: {:?}", other),
        }
    }
}
