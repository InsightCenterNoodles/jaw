use std::{collections::HashMap, ops::RangeInclusive, path::Path};

use crate::tokens::{self, Keyword, Span, Symbol, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TypeID(u32);

#[derive(Debug, Clone, Copy)]
enum Primitive {
    I8,
    I16,
    I32,
    I64,

    U8,
    U16,
    U32,
    U64,

    F32,
    F64,
}

impl Primitive {
    fn is_integer(&self) -> bool {
        match self {
            Primitive::F32 => false,
            Primitive::F64 => false,
            _ => true,
        }
    }
}

#[derive(Debug)]
struct Pack {
    members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
struct Enum {
    ty: Primitive,
    members: Vec<(String, i64)>,
    default: Option<(String, i64)>,
}

#[derive(Debug)]
struct Bitfld {
    ty: TypeID,
    members: Vec<(String, TypeID, RangeInclusive<u32>)>,
}

#[derive(Debug)]
struct Variant {
    ty: TypeID,
    members: Vec<(u64, TypeID)>,
    default: Option<(u64, TypeID)>,
}

#[derive(Debug)]
struct Sequence {
    members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
enum ArrayKind {
    Dynamic {
        size_type: TypeID,
        value_type: TypeID,
    },
    Fixed {
        count: u64,
        value_type: TypeID,
    },
}

#[derive(Debug)]
enum TypeKind {
    Primitive(Primitive),
    Alias(Box<Type>),
    Pack(Pack),
    Enum(Enum),
    Bitfld(Bitfld),
    Variant(Variant),
    Sequence(Sequence),
    DynamicArray(ArrayKind),
    FixedArray(ArrayKind),
}

#[derive(Debug)]
struct Type {
    defined_at: Option<Span>,
    kind: TypeKind,
}

struct TokenReader {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token>>,
}

impl TokenReader {
    fn current_span(&mut self) -> Span {
        self.tokens.peek().map(|x| x.span).unwrap_or_default()
    }

    fn make_unexpected(&self, wanted: &str, found: Token) -> std::io::Error {
        std::io::Error::other(format!("expected {wanted}, found {found:?}"))
    }

    fn demand_next(&mut self) -> std::io::Result<Token> {
        self.tokens
            .next()
            .ok_or_else(|| std::io::Error::other("expected token, found end of file"))
    }

    fn demand_identifier(&mut self) -> std::io::Result<(String, Span)> {
        let token = self.demand_next()?;
        let tokens::TokenKind::Identifier(x) = token.kind else {
            return Err(self.make_unexpected("identifier", token));
        };

        Ok((x, token.span))
    }

    fn demand_newline(&mut self) -> std::io::Result<Span> {
        let token = self.demand_next()?;
        let tokens::TokenKind::Newline = token.kind else {
            return Err(self.make_unexpected("newline", token));
        };

        Ok(token.span)
    }

    fn demand_symbol(&mut self, sym: Symbol) -> std::io::Result<()> {
        let token = self.demand_next()?;
        let tokens::TokenKind::Symbol(x) = token.kind else {
            return Err(self.make_unexpected("symbol", token));
        };

        if x != sym {
            return Err(self.make_unexpected(&format!("symbol '{sym:?}'"), token));
        }

        Ok(())
    }

    fn demand_symbols(&mut self, syms: &[Symbol]) -> std::io::Result<Symbol> {
        let token = self.demand_next()?;
        let tokens::TokenKind::Symbol(x) = token.kind else {
            return Err(self.make_unexpected("symbol", token));
        };

        if !syms.contains(&x) {
            return Err(self.make_unexpected(&format!("one of '{syms:?}'"), token));
        }

        Ok(x)
    }

    fn request_symbols(&mut self, syms: &[Symbol]) -> Option<Symbol> {
        let token = self.tokens.peek()?;

        let tokens::TokenKind::Symbol(x) = token.kind else {
            return None;
        };

        if !syms.contains(&x) {
            return None;
        }

        Some(x)
    }
}

#[derive(Debug)]
pub struct Module {
    type_map: HashMap<String, TypeID>,
    types: HashMap<TypeID, Type>,
    last_tid: u32,
}

impl Module {
    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let mut module = Module {
            type_map: Default::default(),
            types: Default::default(),
            last_tid: 0,
        };

        module.add_builtins();

        let mut tokens = tokens::lex_path(path)?.into_iter().peekable();

        let mut reader = TokenReader { tokens };

        while let Some((keyword, span)) = advance_to_next_kw(&mut reader) {
            match keyword {
                Keyword::Pack => parse_pack(&mut module, &mut reader),
                Keyword::Enum => parse_enum(&mut module, &mut reader),
                Keyword::Bits => parse_bits(&mut module, &mut reader),
                Keyword::Variant => parse_variant(&mut module, &mut reader),
                Keyword::Seq => parse_seq(&mut module, &mut reader),
                Keyword::Alias => todo!(),
                Keyword::Use => todo!(),
                Keyword::As => todo!(),
            }?;
        }

        Ok(module)
    }

    fn install_builtin_type(&mut self, mut name: &str, ty: Type) -> std::io::Result<TypeID> {
        name = name.trim();

        let id = *self.type_map.entry(name.to_string()).or_insert_with(|| {
            let ret = TypeID(self.last_tid);
            self.last_tid += 1;
            ret
        });

        assert!(self.types.insert(id, ty).is_none());

        Ok(id)
    }

    fn install_type_id(&mut self, mut name: &str, span: &Span) -> std::io::Result<TypeID> {
        name = name.trim();

        validate_type_name(name, span)?;

        if let Some(p) = self.type_map.get(name) {
            if let Some(p) = self.lookup(*p) {
                if let Some(p) = p.defined_at {
                    return Err(std::io::Error::other(format!(
                        "error at {span}: multiple definitions of type {name}. Previous definition at: {p}"
                    )));
                } else {
                    return Err(std::io::Error::other(format!(
                        "error at {span}: multiple definitions of type {name}."
                    )));
                }
            }
        }

        Ok(*self.type_map.entry(name.to_string()).or_insert_with(|| {
            let ret = TypeID(self.last_tid);
            self.last_tid += 1;
            ret
        }))
    }

    fn lookup_type_id(&mut self, mut name: &str, span: &Span) -> std::io::Result<TypeID> {
        name = name.trim();

        validate_type_name(name, span)?;

        Ok(*self.type_map.entry(name.to_string()).or_insert_with(|| {
            let ret = TypeID(self.last_tid);
            self.last_tid += 1;
            ret
        }))
    }

    fn lookup(&self, t: TypeID) -> Option<&Type> {
        self.types.get(&t)
    }

    fn add_builtins(&mut self) {
        let mut add_prim = |name: &str, p: Primitive| {
            self.install_builtin_type(
                name,
                Type {
                    defined_at: None,
                    kind: TypeKind::Primitive(p),
                },
            )
            .unwrap();
        };

        add_prim("u8", Primitive::U8);
        add_prim("u16", Primitive::U16);
        add_prim("u32", Primitive::U32);
        add_prim("u64", Primitive::U64);

        add_prim("i8", Primitive::I8);
        add_prim("i16", Primitive::I16);
        add_prim("i32", Primitive::I32);
        add_prim("i64", Primitive::I64);

        add_prim("f32", Primitive::F32);
        add_prim("f64", Primitive::F64);
    }
}

fn validate_type_name(name: &str, span: &Span) -> std::io::Result<()> {
    if let Some(x) = name.chars().next() {
        if !x.is_alphabetic() {
            return Err(std::io::Error::other(format!(
                "error at {span}: types must start with alpha"
            )));
        }
    }

    for c in name.chars() {
        if c.is_whitespace() {
            return Err(std::io::Error::other(format!(
                "error at {span}: types must not contain whitespace"
            )));
        }
    }

    Ok(())
}

fn advance_to_next_kw(reader: &mut TokenReader) -> Option<(Keyword, Span)> {
    loop {
        let next = reader.tokens.next()?;
        match next.kind {
            tokens::TokenKind::Keyword(kw) => {
                return Some((kw, next.span));
            }
            tokens::TokenKind::Newline => {
                continue;
            }
            _ => return None,
        }
    }
}

fn parse_decl_head(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<TypeID> {
    let tid = {
        let (ident, span) = reader.demand_identifier()?;

        module.install_type_id(&ident, &span)?
    };

    Ok(tid)
}

fn parse_decl_head_sub(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<TypeID> {
    reader.demand_symbol(Symbol::Colon)?;

    let (ident, span) = reader.demand_identifier()?;

    module.lookup_type_id(&ident, &span)
}

fn parse_body<F, G>(
    module: &mut Module,
    reader: &mut TokenReader,
    mut f: F,
    mut def: G,
) -> std::io::Result<()>
where
    F: FnMut(&mut Module, &mut TokenReader) -> std::io::Result<()>,
    G: FnMut(&mut Module, &mut TokenReader) -> std::io::Result<()>,
{
    while let Some(sym) = reader.request_symbols(&[Symbol::Minus, Symbol::Equals]) {
        match sym {
            Symbol::Minus => {
                f(module, reader)?;
            }
            Symbol::Equals => {
                def(module, reader)?;
            }
            _ => {
                break;
            }
        }
    }

    Ok(())
}

fn parse_pack(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<()> {
    let at = reader.current_span();

    let tid = parse_decl_head(module, reader)?;

    reader.demand_newline()?;

    let mut members = Vec::<_>::default();

    parse_body(
        module,
        reader,
        |m, r| {
            let mem_name = r.demand_identifier()?;
            r.demand_symbol(Symbol::Colon)?;
            let mem_type = r.demand_identifier()?;

            r.demand_newline()?;

            let mem_type = m.lookup_type_id(&mem_type.0, &mem_type.1)?;

            members.push((mem_name.0, mem_type));

            Ok(())
        },
        |_, _| Err(std::io::Error::other("packs do not have a default")),
    )?;

    let at = at.union(&reader.current_span());

    module.types.insert(
        tid,
        Type {
            defined_at: Some(at),
            kind: TypeKind::Pack(Pack { members }),
        },
    );

    Ok(())
}

fn parse_enum(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<()> {
    let at = reader.current_span();

    let tid = parse_decl_head(module, reader)?;

    let sub_tid = parse_decl_head_sub(module, reader)?;

    reader.demand_newline()?;

    let mut members = Vec::<_>::default();

    let mut default = Option::<_>::None;

    parse_body(
        module,
        reader,
        |m, r| {
            let mem_name = r.demand_identifier()?;

            r.demand_symbol(Symbol::Equals)?;

            let mem_value: i64 = r
                .demand_identifier()?
                .0
                .parse()
                .map_err(|_| std::io::Error::other("enum values must be integers"))?;

            r.demand_newline()?;

            members.push((mem_name.0, mem_value));

            Ok(())
        },
        |_, r| {
            let mem_name = r.demand_identifier()?;

            r.demand_symbol(Symbol::Equals)?;

            let mem_value: i64 = r
                .demand_identifier()?
                .0
                .parse()
                .map_err(|_| std::io::Error::other("enum values must be integers"))?;

            r.demand_newline()?;

            default = Some((mem_name.0, mem_value));

            Ok(())
        },
    )?;

    let at = at.union(&reader.current_span());

    let sub_type = module
        .lookup(sub_tid)
        .ok_or_else(|| std::io::Error::other("enum type unknown"))?;

    let TypeKind::Primitive(ref p) = sub_type.kind else {
        return Err(std::io::Error::other("enum type must be primitive integer"));
    };

    if !p.is_integer() {
        return Err(std::io::Error::other("enum type must be primitive integer"));
    }

    module.types.insert(
        tid,
        Type {
            defined_at: Some(at),
            kind: TypeKind::Enum(Enum {
                ty: *p,
                members,
                default,
            }),
        },
    );

    Ok(())
}

fn parse_bits(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<()> {
    todo!()
}

fn parse_variant(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<()> {
    todo!()
}

fn parse_seq(module: &mut Module, reader: &mut TokenReader) -> std::io::Result<()> {
    todo!()
}
