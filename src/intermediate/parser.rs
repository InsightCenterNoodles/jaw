use std::{
    iter::Peekable,
    io::BufRead,
};

use super::{
    ast::{
        Alias, Bitfld, BitfldMember, DynamicArray, Enum, EnumMember, FixedArray, Import, Module,
        Pack, Sequence, StructMember, Type, TypeKind, TypeName, Variant, VariantMember,
    },
    error::IntermediateError,
    source::{Position, SourceCode, SourceLocation},
};

impl StructMember {
    /// Parses a struct-like member line (used by `pack` and `seq`).
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

        demand_string(source.clone(), &mut iter, ":", place)?;

        let ty = TypeName::from_string(
            source.position(place),
            iter.next()
                .ok_or_else(|| IntermediateError::MalformedMember {
                    position: source.position(place),
                    reason: "missing member type".into(),
                })?
                .1,
        )?;

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                name: name.into(),
                ty,
                defined_at: SourceLocation::new(source, place),
            },
        ))
    }
}

impl EnumMember {
    /// Parses an enum member line (name + `=` value).
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

impl BitfldMember {
    /// Parses a bitfield member line (range + name + `:` type).
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

        let ty = TypeName::from_string(
            source.position(place),
            iter.next()
                .ok_or_else(|| IntermediateError::MalformedMember {
                    position: source.position(place),
                    reason: "missing bitfield member type".into(),
                })?
                .1,
        )?;

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

impl VariantMember {
    /// Parses a variant member line (discriminant + `=>` payload type).
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

        let ty = TypeName::from_string(source.position(value_position), ty)?;

        demand_done(source.clone(), iter)?;

        Ok((
            mem_ty,
            Self {
                ty,
                value,
                defined_at: SourceLocation::new(source, place),
            },
        ))
    }
}

trait HasDefinedAt {
    /// Returns the source location where this item is defined.
    fn defined_at(&self) -> SourceLocation;
}

impl HasDefinedAt for StructMember {
    /// Returns the source location where this member was defined.
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for EnumMember {
    /// Returns the source location where this member was defined.
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for BitfldMember {
    /// Returns the source location where this member was defined.
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl HasDefinedAt for VariantMember {
    /// Returns the source location where this member was defined.
    fn defined_at(&self) -> SourceLocation {
        self.defined_at.clone()
    }
}

impl Module {
    /// Parses a DSL module from an in-memory string.
    pub fn from_string(name: String, source: String) -> Result<Self, IntermediateError> {
        // TODO remove all these clones
        let mut reader = Reader::new(source.clone());

        let mut imports = vec![];
        let mut definitions = vec![];
        let mut saw_declaration = false;

        while let Some(line) = reader.next_line() {
            // parse header
            let mut parts = line.1.split_whitespace();

            let Some(decl_type) = parts.next() else {
                continue;
            };

            if decl_type == "from" {
                if saw_declaration {
                    return Err(IntermediateError::ImportAfterDeclaration {
                        line: reader.code.location(line.0, 0),
                    });
                }
                let import = parse_import_line(&reader.code, line.0, &line.1)?;
                imports.push(import);
                continue;
            }

            let Some(decl_name) = parts.next() else {
                continue;
            };

            saw_declaration = true;

            let extra = line.1.split_once(':').map(|x| x.1.trim());

            let line_number = line.0;

            let defined_at = SourceLocation::new(
                reader.code.clone(),
                Position {
                    line: line_number,
                    column: 0,
                },
            );

            match decl_type {
                "alias" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_alias(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "pack" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_pack(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "seq" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_seq(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "enum" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_enum(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "bits" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_bitfld(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "variant" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_variant(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "fixed_array" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_fixed_array(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
                }),
                "dyn_array" => definitions.push(Type {
                    defined_at: defined_at.clone(),
                    kind: reader.parse_dyn_array(extra, line_number)?,
                    ident: TypeName::from_string(defined_at, decl_name)?,
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
            imports,
            definitions,
        })
    }
}

/// Parses a `from <path> use {Type, ...}` or `from <path> use *` import line.
fn parse_import_line(
    code: &SourceCode,
    line: usize,
    text: &str,
) -> Result<Import, IntermediateError> {
    let mut tokens = split_whitespace_indices(text);

    let Some((_, "from")) = tokens.next() else {
        return Err(IntermediateError::InvalidImport {
            line: code.location(line, 0),
            reason: "import must start with `from`".into(),
        });
    };

    // Check and skip path

    let (path_start_pos, path) = tokens
        .next()
        .ok_or_else(|| IntermediateError::InvalidImport {
            line: code.location(line, 0),
            reason: "missing import path".into(),
        })?;

    if !path.starts_with('"') {
        return Err(IntermediateError::InvalidImport {
            line: code.location(line, 0),
            reason: "import path must start with a double-quotation mark".into(),
        });
    }

    if !path.ends_with('"') {
        loop {
            let (_, path_part) = tokens
                .next()
                .ok_or_else(|| IntermediateError::InvalidImport {
                    line: code.location(line, 0),
                    reason: "malformed import path".into(),
                })?;

            if path_part.ends_with('"') {
                break;
            }
        }
    }

    // we know there is a path now. But to preserve it fully, we only extract now

    let path = text
        .split('"')
        .nth(1)
        .ok_or_else(|| IntermediateError::InvalidImport {
            line: code.location(line, path_start_pos),
            reason: "malformed import path".into(),
        })?;

    let (use_pos, use_kw) = tokens
        .next()
        .ok_or_else(|| IntermediateError::InvalidImport {
            line: code.location(line, 0),
            reason: "missing `use` keyword".into(),
        })?;

    if use_kw != "use" {
        return Err(IntermediateError::InvalidImport {
            line: code.location(line, use_pos),
            reason: format!("expected `use` but found `{use_kw}`"),
        });
    }

    let list_start = use_pos + "use".len();
    let list = text[list_start..].trim();
    let mut import_all = false;
    let mut types = Vec::new();

    if list == "*" {
        import_all = true;
    } else {
        let inner = list
            .strip_prefix('{')
            .and_then(|v| v.strip_suffix('}'))
            .ok_or_else(|| IntermediateError::InvalidImport {
                line: code.location(line, list_start),
                reason: "expected `{...}` type list or `*`".into(),
            })?;

        for item in inner.split(',') {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                continue;
            }
            types.push(TypeName::from_string(
                code.location(line, list_start),
                trimmed,
            )?);
        }

        if types.is_empty() {
            return Err(IntermediateError::InvalidImport {
                line: code.location(line, text.find('{').unwrap_or_default()),
                reason: "import list may not be empty".into(),
            });
        }
    }

    Ok(Import {
        path: path.into(),
        import_all,
        types,
        defined_at: code.location(line, 0),
    })
}

struct Reader {
    code: SourceCode,
    source:
        Peekable<std::iter::Enumerate<std::io::Lines<std::io::BufReader<std::io::Cursor<String>>>>>,
}

impl Reader {
    /// Creates a line-oriented reader over the module source.
    fn new(s: String) -> Self {
        Self {
            code: SourceCode(std::sync::Arc::new(s.clone())),
            source: std::io::BufReader::new(std::io::Cursor::new(s))
                .lines()
                .enumerate()
                .peekable(),
        }
    }

    /// Returns the next non-empty, non-comment line along with its 0-based line number.
    fn next_line(&mut self) -> Option<(usize, String)> {
        loop {
            let x = self.source.next();

            match x {
                Some((num, Ok(ld))) => {
                    // sanitize
                    let mut string = ld.trim();

                    if let Some((a, _)) = string.split_once('#') {
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

    /// Parses a consecutive block of member lines, returning (members, optional default).
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

    /// Peeks for and consumes the next member line (`-` or `>`), if present.
    fn has_member_start(&mut self) -> Option<(usize, String)> {
        let Some((_, Ok(line))) = self.source.peek() else {
            return None;
        };
        if line.starts_with('-') || line.starts_with('>') {
            self.next_line()
        } else {
            None
        }
    }

    /// Parses an `alias` declaration body.
    fn parse_alias(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let Some(extra) = extra else {
            return Err(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "Alias",
            });
        };

        Ok(TypeKind::Alias(Alias {
            other: TypeName::from_string(self.code.location(line, 0), extra.trim())?,
        }))
    }

    /// Parses a `pack` declaration body.
    fn parse_pack(
        &mut self,
        _extra: Option<&str>,
        _line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let (members, default) = self.member_iter(StructMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "pack declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Pack(Pack { members }))
    }

    /// Parses a `seq` declaration body.
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

    /// Parses an `enum` declaration body.
    fn parse_enum(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = TypeName::from_string(
            self.code.location(line, 0),
            extra.ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "enum",
            })?,
        )?;

        let (members, default) = self.member_iter(EnumMember::parse)?;

        Ok(TypeKind::Enum(Enum {
            ty,
            members,
            default,
        }))
    }

    /// Parses a `bits` (bitfield) declaration body.
    fn parse_bitfld(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = TypeName::from_string(
            self.code.location(line, 0),
            extra.ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "bits",
            })?,
        )?;

        let (members, default) = self.member_iter(BitfldMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "bitfield declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Bitfld(Bitfld { ty, members }))
    }

    /// Parses a `variant` declaration body.
    fn parse_variant(
        &mut self,
        extra: Option<&str>,
        line: usize,
    ) -> Result<TypeKind, IntermediateError> {
        let ty = TypeName::from_string(
            self.code.location(line, 0),
            extra.ok_or(IntermediateError::MissingDeclarationDetail {
                line: self.code.location(line, 0),
                kind: "variant",
            })?,
        )?;

        let (members, default) = self.member_iter(VariantMember::parse)?;

        if let Some(member) = default {
            return Err(IntermediateError::MalformedMember {
                position: member.defined_at().clone(),
                reason: "variant declarations do not support default members".into(),
            });
        }

        Ok(TypeKind::Variant(Variant { ty, members }))
    }

    /// Parses a `fixed_array` declaration body.
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
            value_type: TypeName::from_string(self.code.location(line, 0), parts.1.trim())?,
        }))
    }

    /// Parses a `dyn_array` declaration body.
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

        let size_type = TypeName::from_string(self.code.location(line, 0), parts.0.trim())?;
        let value_type = TypeName::from_string(self.code.location(line, 0), parts.1.trim())?;

        Ok(TypeKind::DynamicArray(DynamicArray {
            size_type,
            value_type,
        }))
    }
}

#[derive(Debug, Clone, Copy)]
enum MemberType {
    Normal,
    Default,
}

/// Returns the raw pointer address of a string slice (used for offset computation).
fn addr_of(s: &str) -> usize {
    s.as_ptr() as usize
}

/// Splits a string on whitespace while also returning byte offsets for each token.
fn split_whitespace_indices(s: &str) -> impl Iterator<Item = (usize, &str)> {
    s.split_whitespace()
        .map(move |sub| (addr_of(sub) - addr_of(s), sub))
}

/// Tokenizes a member line into `(Position, token)` pairs.
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

/// Consumes the member prefix token (`-` normal, `>` default) and returns its meaning.
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

/// Expects the next token to match `text` and errors otherwise.
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

/// Ensures the member tokenizer has no extra trailing tokens.
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
