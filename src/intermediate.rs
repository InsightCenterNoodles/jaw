use std::{fmt::Display, io::BufRead, iter::Peekable};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct SourceCode(pub std::sync::Arc<String>);

impl SourceCode {
    pub fn location(&self, line: usize, column: usize) -> SourceLocation {
        SourceLocation {
            source: self.clone(),
            position: Position { line, column },
        }
    }

    pub fn position(&self, position: Position) -> SourceLocation {
        SourceLocation {
            source: self.clone(),
            position,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    source: SourceCode,
    position: Position,
}

impl SourceLocation {
    pub fn new(source: SourceCode, position: Position) -> Self {
        Self { source, position }
    }
}

impl Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.source.0.lines().nth(self.position.line) {
            if let Some((a, b)) = line.split_at_checked(self.position.column) {
                return write!(f, "line {}: {}↪{}", self.position.line, a, b);
            }
        }

        write!(f, "unknown location")
    }
}

#[derive(Debug, Error)]
pub enum IntermediateError {
    #[error("unsupported declaration type `{decl_type}` at {line}")]
    UnsupportedDeclaration {
        line: SourceLocation,
        decl_type: String,
    },
    #[error("{kind} declaration requires additional detail at {line}")]
    MissingDeclarationDetail {
        line: SourceLocation,
        kind: &'static str,
    },
    #[error("malformed member at {position}: {reason}")]
    MalformedMember {
        position: SourceLocation,
        reason: String,
    },
    #[error("invalid number `{value}` at {position}: {source}")]
    InvalidNumber {
        position: SourceLocation,
        value: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("duplicate default member at line {line}")]
    DuplicateDefault { line: SourceLocation },
    #[error("invalid array specification at line {line}")]
    InvalidArraySpec { line: SourceLocation },
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct TypeName(std::sync::Arc<String>);

impl TypeName {
    #[allow(unused)]
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for TypeName {
    fn from(value: &str) -> Self {
        Self(std::sync::Arc::new(value.trim().into()))
    }
}

impl From<String> for TypeName {
    fn from(value: String) -> Self {
        Self(std::sync::Arc::new(value.trim().into()))
    }
}

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub ty: TypeName,
    pub defined_at: SourceLocation,
}

impl StructMember {
    fn parse(
        source: SourceCode,
        input: (usize, String),
    ) -> Result<(MemberType, Self), IntermediateError> {
        let mut iter = make_mem_split(&input);
        let fallback = Position {
            line: input.0,
            column: 0,
        };
        let mem_ty = consume_member_start(source.clone(), &mut iter, fallback)?;

        let (place, name) = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(fallback),
                reason: "missing member name".into(),
            })?;

        //dbg!(place, name);

        demand_string(source.clone(), &mut iter, ":", place)?;

        let ty = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(place),
                reason: "missing member type".into(),
            })?
            .1
            .to_string();

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                name: name.into(),
                ty: ty.into(),
                defined_at: SourceLocation::new(source, place),
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
    pub defined_at: SourceLocation,
}

impl EnumMember {
    fn parse(
        source: SourceCode,
        input: (usize, String),
    ) -> Result<(MemberType, Self), IntermediateError> {
        let mut iter = make_mem_split(&input);
        let fallback = Position {
            line: input.0,
            column: 0,
        };
        let mem_ty = consume_member_start(source.clone(), &mut iter, fallback)?;

        //dbg!(mem_ty);

        let (place, name) = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(fallback),
                reason: "missing enum member name".into(),
            })?;

        demand_string(source.clone(), &mut iter, "=", place)?;

        let (value_position, value_raw) =
            iter.next()
                .ok_or_else(|| IntermediateError::MalformedMember {
                    position: source.position(place),
                    reason: "missing enum member value".into(),
                })?;

        let value = value_raw
            .parse()
            .map_err(|s| IntermediateError::InvalidNumber {
                position: source.position(value_position),
                value: value_raw.to_string(),
                source: s,
            })?;

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                name: name.into(),
                value,
                defined_at: SourceLocation::new(source, place),
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
    pub defined_at: SourceLocation,
}

impl BitfldMember {
    fn parse(
        source: SourceCode,
        input: (usize, String),
    ) -> Result<(MemberType, Self), IntermediateError> {
        let mut iter = make_mem_split(&input);
        let fallback = Position {
            line: input.0,
            column: 0,
        };
        let mem_ty = consume_member_start(source.clone(), &mut iter, fallback)?;

        let range = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(fallback),
                reason: "missing bitfield range".into(),
            })?
            .1
            .to_string();

        let (place, name) = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(fallback),
                reason: "missing bitfield member name".into(),
            })?;

        demand_string(source.clone(), &mut iter, ":", place)?;

        let ty = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(place),
                reason: "missing bitfield member type".into(),
            })?
            .1
            .into();

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                name: name.into(),
                ty,
                range,
                defined_at: SourceLocation::new(source, place),
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
    pub defined_at: SourceLocation,
}

impl VariantMember {
    fn parse(
        source: SourceCode,
        input: (usize, String),
    ) -> Result<(MemberType, Self), IntermediateError> {
        let mut iter = make_mem_split(&input);
        let fallback = Position {
            line: input.0,
            column: 0,
        };
        let mem_ty = consume_member_start(source.clone(), &mut iter, fallback)?;

        let (value_position, raw_value) =
            iter.next()
                .ok_or_else(|| IntermediateError::MalformedMember {
                    position: source.position(fallback),
                    reason: "missing variant discriminant".into(),
                })?;
        let value = raw_value
            .parse()
            .map_err(|s| IntermediateError::InvalidNumber {
                position: source.position(value_position),
                value: raw_value.to_string(),
                source: s,
            })?;

        demand_string(source.clone(), &mut iter, "=>", value_position)?;

        let (place, ty) = iter
            .next()
            .ok_or_else(|| IntermediateError::MalformedMember {
                position: source.position(value_position),
                reason: "missing variant type".into(),
            })?;

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                ty: ty.into(),
                value,
                defined_at: SourceLocation::new(source, place),
            },
        ))
    }
}

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub ty: TypeName,
    pub members: Vec<VariantMember>,
}

trait HasDefinedAt {
    fn defined_at(&self) -> SourceLocation;
}

impl HasDefinedAt for StructMember {
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for EnumMember {
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for BitfldMember {
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for VariantMember {
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
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
    pub defined_at: SourceLocation,
    pub kind: TypeKind,
}

#[derive(Debug)]
pub struct Module {
    pub name: String,

    pub source: String,

    pub definitions: Vec<Type>,
}

impl Module {
    pub fn from_string(name: String, source: String) -> Result<Self, IntermediateError> {
        // TODO remove all these clones
        let mut reader = Reader::new(source.clone());

        let mut definitions = vec![];

        while let Some(line) = reader.next_line() {
            //dbg!(&line);

            // parse header
            let mut parts = line.1.split_whitespace();

            let Some(decl_type) = parts.next() else {
                continue;
            };

            let Some(decl_name) = parts.next() else {
                continue;
            };

            let decl_name = decl_name.into();

            let extra = line.1.split_once(':').map(|x| x.1.trim());

            //dbg!(extra);

            let line_number = line.0;

            match decl_type {
                "alias" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_alias(extra, line_number)?,
                    ident: decl_name,
                }),
                "pack" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_pack(extra, line_number)?,
                    ident: decl_name,
                }),
                "seq" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_seq(extra, line_number)?,
                    ident: decl_name,
                }),
                "enum" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_enum(extra, line_number)?,
                    ident: decl_name,
                }),
                "bits" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_bitfld(extra, line_number)?,
                    ident: decl_name,
                }),
                "variant" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_variant(extra, line_number)?,
                    ident: decl_name,
                }),
                "fixed_array" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_fixed_array(extra, line_number)?,
                    ident: decl_name,
                }),
                "dyn_array" => definitions.push(Type {
                    defined_at: SourceLocation::new(
                        reader.code.clone(),
                        Position {
                            line: line_number,
                            column: 0,
                        },
                    ),
                    kind: reader.parse_dyn_array(extra, line_number)?,
                    ident: decl_name,
                }),
                _ => {
                    return Err(IntermediateError::UnsupportedDeclaration {
                        line: reader.code.location(line_number, 0),
                        decl_type: decl_type.to_owned(),
                    });
                }
            }
        }

        Ok(Module {
            name,
            source,
            definitions,
        })
    }
}

struct Reader {
    code: SourceCode,
    source:
        Peekable<std::iter::Enumerate<std::io::Lines<std::io::BufReader<std::io::Cursor<String>>>>>,
}

impl Reader {
    fn new(s: String) -> Self {
        Self {
            code: SourceCode(std::sync::Arc::new(s.clone())),
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

    fn member_iter<Func, U>(
        &mut self,
        mut f: Func,
    ) -> Result<(Vec<U>, Option<U>), IntermediateError>
    where
        Func: FnMut(SourceCode, (usize, String)) -> Result<(MemberType, U), IntermediateError>,
        U: HasDefinedAt,
    {
        let mut ret = vec![];
        let mut def = None;

        loop {
            let Some(l) = self.has_member_start() else {
                break;
            };

            let item = f(self.code.clone(), l)?;
            let (member_type, parsed) = item;

            match member_type {
                MemberType::Normal => ret.push(parsed),
                MemberType::Default => {
                    if def.is_some() {
                        return Err(IntermediateError::DuplicateDefault {
                            line: parsed.defined_at().clone(),
                        });
                    }
                    def = Some(parsed);
                }
            }
        }

        Ok((ret, def))
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

    fn parse_alias(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let Some(extra) = extra else {
            return Err(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "Aliass",
            });
        };

        Ok(TypeKind::Alias(Alias {
            other: extra.trim().into(),
        }))
    }

    fn parse_pack(
        &mut self,
        _extra: Option<&str>,
        _line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let (members, default) = self.member_iter(StructMember::parse)?;

        //dbg!(&members);

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "pack declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Pack(Pack { members }))
    }

    fn parse_seq(
        &mut self,
        _extra: Option<&str>,
        _line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let (members, default) = self.member_iter(StructMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "sequence declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Sequence(Sequence { members }))
    }

    fn parse_enum(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = extra
            .ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "enum",
            })?
            .into();

        let (members, default) = self.member_iter(EnumMember::parse)?;

        Ok(TypeKind::Enum(Enum {
            ty,
            members,
            default,
        }))
    }

    fn parse_bitfld(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = extra
            .ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "bits",
            })?
            .into();

        let (members, default) = self.member_iter(BitfldMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "bitfield declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Bitfld(Bitfld { ty, members }))
    }

    fn parse_variant(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = extra
            .ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "variant",
            })?
            .into();

        let (members, default) = self.member_iter(VariantMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "variant declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Variant(Variant { ty, members }))
    }

    fn parse_fixed_array(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let spec = extra.ok_or(IntermediateError::MissingDeclarationDetail {
            line: self.code.location(line, 0),
            kind: "fixed_array",
        })?;
        let parts = spec
            .split_once('*')
            .ok_or(IntermediateError::InvalidArraySpec {
                line: self.code.location(line, 0),
            })?;

        //dbg!(parts);

        let count_str = parts.0.trim();
        let count = count_str
            .parse()
            .map_err(|source| IntermediateError::InvalidNumber {
                position: self.code.location(line, 0),
                value: count_str.to_string(),
                source,
            })?;

        Ok(TypeKind::FixedArray(FixedArray {
            count,
            value_type: parts.1.trim().into(),
        }))
    }

    fn parse_dyn_array(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let spec = extra.ok_or(IntermediateError::MissingDeclarationDetail {
            line: self.code.location(line, 0),
            kind: "dyn_array",
        })?;
        let parts = spec
            .split_once('*')
            .ok_or(IntermediateError::InvalidArraySpec {
                line: self.code.location(line, 0),
            })?;

        Ok(TypeKind::DynamicArray(DynamicArray {
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
    code: SourceCode,
    iter: &mut impl Iterator<Item = (Position, &'a str)>,
    fallback: Position,
) -> Result<MemberType, IntermediateError> {
    match iter.next() {
        Some((_, "-")) => Ok(MemberType::Normal),
        Some((_, ">")) => Ok(MemberType::Default),
        Some((position, other)) => Err(IntermediateError::MalformedMember {
            position: code.position(position),
            reason: format!("expected `-` or `>` but found `{other}`"),
        }),
        None => Err(IntermediateError::MalformedMember {
            position: code.position(fallback),
            reason: "missing member prefix".into(),
        }),
    }
}

fn demand_string<'a>(
    code: SourceCode,
    iter: &mut impl Iterator<Item = (Position, &'a str)>,
    text: &str,
    fallback: Position,
) -> Result<(), IntermediateError> {
    match iter.next() {
        Some((_, x)) if x == text => Ok(()),
        Some((position, other)) => Err(IntermediateError::MalformedMember {
            position: code.position(position),
            reason: format!("expected `{text}` but found `{other}`"),
        }),
        None => Err(IntermediateError::MalformedMember {
            position: code.position(fallback),
            reason: format!("expected `{text}`"),
        }),
    }
}

fn demand_done<'a>(
    code: SourceCode,
    mut iter: impl Iterator<Item = (Position, &'a str)>,
) -> Result<(), IntermediateError> {
    if let Some((position, extra)) = iter.next() {
        Err(IntermediateError::MalformedMember {
            position: code.position(position),
            reason: format!("unexpected extra token `{extra}`"),
        })
    } else {
        Ok(())
    }
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
        let source = include_str!("../assets/example.jaw");

        let m = Module::from_string("file".into(), source.into()).expect("parse module");

        let m: HashMap<_, _> = m
            .definitions
            .into_iter()
            .map(|x| (x.ident.clone(), x))
            .collect();

        assert_eq!(m.len(), 15);

        match &m[&"MyPOD".into()].kind {
            TypeKind::Pack(pack) => {
                assert_eq!(
                    struct_sig(&pack.members),
                    vec![("a_thing", "u8"), ("b_thing", "u64")]
                );
            }
            other => panic!("MyPOD parsed as unexpected kind: {:?}", other),
        }

        match &m[&"MyOtherPOD".into()].kind {
            TypeKind::Pack(pack) => {
                assert_eq!(
                    struct_sig(&pack.members),
                    vec![("first", "MyPOD"), ("second", "FixedString")]
                );
            }
            other => panic!("MyOtherPOD parsed as unexpected kind: {:?}", other),
        }

        match &m[&"PlainEnum".into()].kind {
            TypeKind::Enum(e) => {
                assert_eq!(e.ty.as_str(), "u8");
                assert!(e.default.is_none());
                assert_eq!(enum_sig(&e.members), vec![("F1", 0), ("F2", 1)]);
            }
            other => panic!("PlainEnum parsed as unexpected kind: {:?}", other),
        }

        match &m[&"BetterEnum".into()].kind {
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

        match &m[&"MyFlags".into()].kind {
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

        match &m[&"SmallSeq".into()].kind {
            TypeKind::Sequence(seq) => {
                assert_eq!(struct_sig(&seq.members), vec![("list", "Data")]);
            }
            other => panic!("SmallSeq parsed as unexpected kind: {:?}", other),
        }

        match &m[&"MyPODFixedList".into()].kind {
            TypeKind::FixedArray(arr) => {
                assert_eq!(arr.count, 8);
                assert_eq!(arr.value_type.as_str(), "MyPOD");
            }
            other => panic!("MyPODFixedList parsed as unexpected kind: {:?}", other),
        }

        match &m[&"MyOtherPODDynList".into()].kind {
            TypeKind::DynamicArray(arr) => {
                assert_eq!(arr.size_type.as_str(), "u16");
                assert_eq!(arr.value_type.as_str(), "MyOtherPOD");
            }
            other => panic!("MyOtherPODDynList parsed as unexpected kind: {:?}", other),
        }

        match &m[&"ComplexSeq".into()].kind {
            TypeKind::Sequence(seq) => {
                assert_eq!(
                    struct_sig(&seq.members),
                    vec![("flags", "MyFlags"), ("list", "MyPODFixedList"), ("other_list", "MyOtherPODDynList")]
                );
            }
            other => panic!("ComplexSeq parsed as unexpected kind: {:?}", other),
        }

        match &m[&"Root".into()].kind {
            TypeKind::Sequence(seq) => {
                assert_eq!(
                    struct_sig(&seq.members),
                    vec![("name", "ShortString"), ("var", "MyVariant")]
                );
            }
            other => panic!("Root parsed as unexpected kind: {:?}", other),
        }

        match &m[&"FixedString".into()].kind {
            TypeKind::FixedArray(arr) => {
                assert_eq!(arr.count, 4);
                assert_eq!(arr.value_type.as_str(), "u8");
            }
            other => panic!("FixedString parsed as unexpected kind: {:?}", other),
        }

        match &m[&"ShortString".into()].kind {
            TypeKind::DynamicArray(arr) => {
                assert_eq!(arr.size_type.as_str(), "u8");
                assert_eq!(arr.value_type.as_str(), "u8");
            }
            other => panic!("ShortString parsed as unexpected kind: {:?}", other),
        }

        match &m[&"Data".into()].kind {
            TypeKind::DynamicArray(arr) => {
                assert_eq!(arr.size_type.as_str(), "u8");
                assert_eq!(arr.value_type.as_str(), "f32");
            }
            other => panic!("Data parsed as unexpected kind: {:?}", other),
        }

        match &m[&"MyVariant".into()].kind {
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
}
