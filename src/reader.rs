use std::{
    io::{BufRead, Read},
    iter::Peekable,
    ops::RangeInclusive,
    path::PathBuf,
};

#[derive(Debug)]
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
    pub range: RangeInclusive<u32>,
    pub defined_at: Position,
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
    pub fn from_stream(source: String, r: &mut impl Read) -> Option<Self> {
        let mut reader = Reader::new(
            std::io::BufReader::new(r)
                .lines()
                .filter_map(|x| x.ok())
                .enumerate(),
        );

        let mut module = Module {
            source,
            definitions: vec![],
        };

        while let Some(line) = reader.next_line() {
            // parse header
            let mut parts = line.1.split_whitespace().peekable();

            let Some(decl_type) = parts.next() else {
                continue;
            };

            let Some(decl_name) = parts.next() else {
                continue;
            };

            let extra = if let Some(&maybe_colon) = parts.peek() {
                if maybe_colon == ":" {
                    parts.next()?;
                    parts.next()
                } else {
                    None
                }
            } else {
                None
            };

            match decl_type {
                "pack" => module.definitions.push(Type {
                    defined_at: Position {
                        line: line.0,
                        column: 0,
                    },
                    kind: reader.parse_pack(extra)?,
                    ident: decl_name.to_owned(),
                }),
                x => todo!(),
            }
        }

        todo!()
    }
}

struct Reader<T: Iterator> {
    source: Peekable<T>,
}

impl<T> Reader<T>
where
    T: Iterator<Item = (usize, String)>,
{
    fn new(iter: T) -> Self {
        Self {
            source: iter.peekable(),
        }
    }

    fn next_line(&mut self) -> Option<(usize, String)> {
        loop {
            let x = self.source.next();

            match x {
                Some((num, ld)) => {
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
                None => return None,
            }
        }
    }

    fn make_aline_iter(s: &(usize, String)) -> impl Iterator<Item = (Position, &str)> {
        split_whitespace_indices(&s.1).map(|x| {
            (
                Position {
                    line: s.0,
                    column: x.0,
                },
                x.1,
            )
        })
    }

    fn has_member_start(&mut self) -> Option<(usize, String)> {
        let l = self.source.peek()?;
        if l.1.starts_with("-") {
            self.next_line()
        } else {
            None
        }
    }

    fn consume_member_start<'a>(iter: &mut impl Iterator<Item = (Position, &'a str)>) {
        Self::consume_string(iter, "-");
    }

    fn consume_string<'a>(iter: &mut impl Iterator<Item = (Position, &'a str)>, text: &str) {
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

    fn parse_struct_member(&mut self) -> Option<StructMember> {
        let Some(next) = self.has_member_start() else {
            return None;
        };

        let mut parts = Self::make_aline_iter(&next);

        Self::consume_member_start(&mut parts);

        let name = parts.next()?;

        Self::consume_string(&mut parts, ":");

        let ty = parts.next()?;

        Self::demand_done(parts);

        Some(StructMember {
            name: name.1.into(),
            ty: ty.1.into(),
            defined_at: name.0,
        })
    }

    fn parse_pack<'a, I>(&mut self, _extra: Option<&str>) -> Option<TypeKind> {
        let members = std::iter::from_fn(|| self.parse_struct_member()).collect();

        Some(TypeKind::Pack(Pack { members }))
    }

    fn parse_seq<'a, I>(&mut self, _extra: Option<&str>) -> Option<TypeKind> {
        let members = std::iter::from_fn(|| self.parse_struct_member()).collect();

        Some(TypeKind::Sequence(Sequence { members }))
    }

    fn parse_enum<'a, I>(&mut self, extra: Option<&str>) -> Option<TypeKind> {
        let ty = extra.unwrap();

        //let members = std::iter::from_fn(|| self.parse_struct_member()).collect();

        //Some(TypeKind::Sequence(Sequence { members }))
    }
}

fn addr_of(s: &str) -> usize {
    s.as_ptr() as usize
}

fn split_whitespace_indices(s: &str) -> impl Iterator<Item = (usize, &str)> {
    s.split_whitespace()
        .map(move |sub| (addr_of(sub) - addr_of(s), sub))
}
