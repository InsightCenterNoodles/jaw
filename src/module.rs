use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::RangeInclusive,
    path::Path,
};

use crate::tokenreader::TokenReader;
use crate::tokens::{self, Keyword, Span, Symbol, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeID(u32);

impl std::fmt::Display for TypeID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeID({})", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Primitive {
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
        !matches!(self, Primitive::F32 | Primitive::F64)
    }

    fn bit_width(&self) -> Option<u32> {
        match self {
            Primitive::I8 | Primitive::U8 => Some(8),
            Primitive::I16 | Primitive::U16 => Some(16),
            Primitive::I32 | Primitive::U32 => Some(32),
            Primitive::I64 | Primitive::U64 => Some(64),
            Primitive::F32 | Primitive::F64 => None,
        }
    }

    fn int_bounds(&self) -> Option<(i128, i128)> {
        match self {
            Primitive::U8 => Some((u8::MIN as i128, u8::MAX as i128)),
            Primitive::U16 => Some((u16::MIN as i128, u16::MAX as i128)),
            Primitive::U32 => Some((u32::MIN as i128, u32::MAX as i128)),
            Primitive::U64 => Some((u64::MIN as i128, u64::MAX as i128)),
            Primitive::I8 => Some((i8::MIN as i128, i8::MAX as i128)),
            Primitive::I16 => Some((i16::MIN as i128, i16::MAX as i128)),
            Primitive::I32 => Some((i32::MIN as i128, i32::MAX as i128)),
            Primitive::I64 => Some((i64::MIN as i128, i64::MAX as i128)),
            Primitive::F32 | Primitive::F64 => None,
        }
    }
}

impl std::fmt::Display for Primitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Primitive::I8 => "i8",
                Primitive::I16 => "i16",
                Primitive::I32 => "i32",
                Primitive::I64 => "i64",
                Primitive::U8 => "u8",
                Primitive::U16 => "u16",
                Primitive::U32 => "u32",
                Primitive::U64 => "u64",
                Primitive::F32 => "f32",
                Primitive::F64 => "f64",
            }
        )
    }
}

#[derive(Debug)]
pub struct Pack {
    pub members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
pub struct Enum {
    pub ty: Primitive,
    pub members: Vec<(String, i64)>,
    pub default: Option<(String, i64)>,
}

#[derive(Debug)]
pub struct Bitfld {
    pub ty: TypeID,
    pub members: Vec<(String, TypeID, RangeInclusive<u32>)>,
}

#[derive(Debug)]
pub struct Variant {
    pub ty: Primitive,
    pub members: Vec<(u64, TypeID)>,
    pub default: Option<(u64, TypeID)>,
}

#[derive(Debug)]
pub struct Sequence {
    pub members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
pub enum ArrayKind {
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
pub enum TypeKind {
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
pub struct Type {
    pub defined_at: Span,
    pub kind: TypeKind,
}

// MARK: Error
#[derive(Debug, thiserror::Error)]
pub enum ModuleBuildError {
    #[error("IO error")]
    IO(#[from] std::io::Error),
    #[error("Lexer error")]
    Lex(#[from] crate::tokens::LexError),

    #[error("Unknown type ID {0}")]
    UnknownType(TypeID),

    #[error("Incomplete type {0} first noted at {1:?}")]
    BadLookup(String, Span),

    #[error("Pack {0} contains non-POD member at {1}")]
    NonPODInPack(String, Span),

    #[error("Underlying type of enum {0} must be an integer (found {1})")]
    NonIntEnum(String, Span, Primitive),

    #[error("Enum value '{name}'={value} out of range [{min}..={max}] at {at}")]
    EnumValueOutOfRange {
        name: String,
        at: Span,
        value: i128,
        min: i128,
        max: i128,
    },

    #[error("Underlying type of bitfield {0} must be an integer (found {1})")]
    NonIntBitfld(String, Span, String),

    #[error("bitfield range for '{name}' out of bounds (end {end} >= width {width}) at {at}")]
    BitfldVOutOfRange {
        name: String,
        at: Span,
        end: u32,
        width: u32,
    },

    #[error("Underlying type of variant {0} must be an integer (found {1})")]
    NonIntVariant(String, Span, Primitive),

    #[error("variant discriminant {value} out of range (max {max}) at {at}")]
    VariantValueOutOfRange { at: Span, value: i128, max: i128 },

    #[error("[I * M] requires M to be POD at {0}")]
    FixedArrayRequiresPOD(Span),

    #[error("{n_m}: size type must be a primitive integer at {at}")]
    ArraySizeTypeNotInteger { n_m: &'static str, at: Span },

    #[error("{i_m}: I must be positive at {at}")]
    ArrayCountNotPositive { i_m: &'static str, at: Span },

    #[error("invalid type name at {at}: {reason}")]
    InvalidTypeName { at: Span, reason: String },

    #[error("duplicate definition of type {name} at {at}{prev}")]
    DuplicateTypeDefinition {
        name: String,
        at: Span,
        prev: String,
    },

    #[error("bitfield member '{field}' must be primitive or enum at {at}")]
    BitfieldMemberTypeInvalid { field: String, at: Span },

    #[error("{kind} declarations do not have a default at {at}")]
    UnexpectedDefault { kind: &'static str, at: Span },

    #[error("invalid bit range start={start} end={end} at {at}")]
    InvalidBitRange { at: Span, start: u32, end: u32 },

    #[error("enum value must be an integer at {0}")]
    EnumValueNotInteger(Span),

    #[error("variant value must be non-negative at {0}")]
    VariantValueNegative(Span),

    #[error("enum underlying type for {0} must be primitive (found {1}) at {2}")]
    NonPrimitiveEnumBase(String, String, Span),

    #[error("variant underlying type for {0} must be primitive (found {1}) at {2}")]
    NonPrimitiveVariantBase(String, String, Span),

    #[error("unexpected token: expected {expected}, found {found:?}")]
    UnexpectedToken {
        expected: String,
        found: crate::tokens::Token,
    },

    #[error("unexpected end of file")]
    UnexpectedEOF,

    #[error("internal error: {0}")]
    Internal(String),
}

type Result<T> = std::result::Result<T, ModuleBuildError>;

// MARK: Module

#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub type_map: HashMap<String, (TypeID, Span)>,
    pub inv_type_map: HashMap<TypeID, String>,
    pub types: HashMap<TypeID, Type>,
    pub type_order: Vec<TypeID>,
}

impl Module {
    pub fn lookup(&self, t: TypeID) -> Option<&Type> {
        self.types.get(&t)
    }
}

#[derive(Debug)]
pub struct PartialModule {
    pub name: String,
    type_map: HashMap<String, (TypeID, Span)>,
    types: HashMap<TypeID, Type>,
    last_tid: u32,
}

impl PartialModule {
    pub fn from_file(path: &Path) -> Result<Self> {
        let file_stem = path
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("module")
            .to_string();

        let mut module = PartialModule {
            name: file_stem,
            type_map: Default::default(),
            types: Default::default(),
            last_tid: 0,
        };

        module.add_builtins();

        let tokens = tokens::lex_path(path)?.into_iter().peekable();

        let mut reader = TokenReader::new(tokens);

        while let Some((keyword, _)) = reader.scan_to_next_kw() {
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

        module.validate()?;

        Ok(module)
    }

    pub fn from_string(source: &str) -> Result<Self> {
        let mut module = PartialModule {
            name: "string".into(),
            type_map: Default::default(),
            types: Default::default(),
            last_tid: 0,
        };

        module.add_builtins();

        let tokens = tokens::lex_str(source)?.into_iter().peekable();

        let mut reader = TokenReader::new(tokens);

        while let Some((keyword, _)) = reader.scan_to_next_kw() {
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

        module.validate()?;

        Ok(module)
    }

    pub fn compile(self) -> Module {
        // Build dependency graph: dep -> dependents, and in-degree counts for Kahn's algorithm
        let types = self.all_types();

        let mut indegree: HashMap<TypeID, usize> = HashMap::new();
        let mut dependents: HashMap<TypeID, Vec<TypeID>> = HashMap::new();

        for &id in types.keys() {
            indegree.entry(id).or_insert(0);
            dependents.entry(id).or_default();
        }

        // Helper to collect direct dependencies of a type
        fn collect_deps(ty: &Type) -> Vec<TypeID> {
            let mut deps: Vec<TypeID> = Vec::new();
            match &ty.kind {
                TypeKind::Primitive(_) => {}
                TypeKind::Alias(inner) => {
                    // Recurse into the aliased type definition
                    collect_deps_into(inner, &mut deps);
                }
                TypeKind::Pack(Pack { members }) => {
                    for (_, tid) in members {
                        deps.push(*tid);
                    }
                }
                TypeKind::Enum(_) => {}
                TypeKind::Bitfld(Bitfld { ty, members }) => {
                    deps.push(*ty);
                    for (_, mt, _) in members {
                        deps.push(*mt);
                    }
                }
                TypeKind::Variant(Variant {
                    ty: _ty,
                    members,
                    default,
                }) => {
                    for (_, vt) in members {
                        deps.push(*vt);
                    }
                    if let Some((_, dt)) = default {
                        deps.push(*dt);
                    }
                }
                TypeKind::Sequence(Sequence { members }) => {
                    for (_, tid) in members {
                        deps.push(*tid);
                    }
                }
                TypeKind::DynamicArray(kind) | TypeKind::FixedArray(kind) => match kind {
                    ArrayKind::Dynamic {
                        size_type,
                        value_type,
                    } => {
                        deps.push(*size_type);
                        deps.push(*value_type);
                    }
                    ArrayKind::Fixed {
                        count: _,
                        value_type,
                    } => {
                        deps.push(*value_type);
                    }
                },
            }

            deps
        }

        fn collect_deps_into(ty: &Type, out: &mut Vec<TypeID>) {
            match &ty.kind {
                TypeKind::Primitive(_) => {}
                TypeKind::Alias(inner) => collect_deps_into(inner, out),
                TypeKind::Pack(Pack { members }) => {
                    for (_, tid) in members {
                        out.push(*tid);
                    }
                }
                TypeKind::Enum(_) => {}
                TypeKind::Bitfld(Bitfld { ty, members }) => {
                    out.push(*ty);
                    for (_, mt, _) in members {
                        out.push(*mt);
                    }
                }
                TypeKind::Variant(Variant {
                    ty: _ty,
                    members,
                    default,
                }) => {
                    for (_, vt) in members {
                        out.push(*vt);
                    }
                    if let Some((_, dt)) = default {
                        out.push(*dt);
                    }
                }
                TypeKind::Sequence(Sequence { members }) => {
                    for (_, tid) in members {
                        out.push(*tid);
                    }
                }
                TypeKind::DynamicArray(kind) | TypeKind::FixedArray(kind) => match kind {
                    ArrayKind::Dynamic {
                        size_type,
                        value_type,
                    } => {
                        out.push(*size_type);
                        out.push(*value_type);
                    }
                    ArrayKind::Fixed {
                        count: _,
                        value_type,
                    } => {
                        out.push(*value_type);
                    }
                },
            }
        }

        for (&id, ty) in types {
            let mut seen: HashSet<TypeID> = HashSet::new();
            for dep in collect_deps(ty) {
                if dep == id {
                    continue; // ignore self-dependency
                }
                if !types.contains_key(&dep) {
                    // Referenced type not present (should not happen after validate); skip defensively
                    continue;
                }
                if seen.insert(dep) {
                    dependents.entry(dep).or_default().push(id);
                    *indegree.entry(id).or_insert(0) += 1;
                }
            }
        }

        let mut order: Vec<TypeID> = Vec::with_capacity(indegree.len());
        let mut q: VecDeque<TypeID> = indegree
            .iter()
            .filter_map(|(&id, &deg)| if deg == 0 { Some(id) } else { None })
            .collect();

        while let Some(n) = q.pop_front() {
            order.push(n);
            if let Some(nexts) = dependents.get(&n) {
                for &m in nexts {
                    if let Some(d) = indegree.get_mut(&m) {
                        *d -= 1;
                        if *d == 0 {
                            q.push_back(m);
                        }
                    }
                }
            }
        }

        // If a cycle exists, append remaining nodes in arbitrary order to complete the list
        if order.len() < indegree.len() {
            for &id in indegree.keys() {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
        }

        let map: HashMap<_, _> = self
            .type_map
            .iter()
            .map(|(a, b)| (b.0, a.clone()))
            .collect();

        Module {
            name: self.name,
            type_map: self.type_map,
            types: self.types,
            type_order: order,
            inv_type_map: map,
        }
    }

    pub fn validate(&self) -> Result<()> {
        // helper: POD predicate with recursion guard
        fn is_pod(module: &PartialModule, tid: TypeID, seen: &mut HashSet<TypeID>) -> Result<bool> {
            if !seen.insert(tid) {
                // recursive reference; treat as non-POD to be conservative
                return Ok(false);
            }
            let t = module.lookup(tid)?;

            let res = match &t.kind {
                TypeKind::Primitive(_) => true,
                TypeKind::Enum(_) => true,
                TypeKind::Bitfld(_) => true,
                TypeKind::FixedArray(ArrayKind::Dynamic {
                    size_type: _,
                    value_type: _,
                }) => false,
                TypeKind::FixedArray(ArrayKind::Fixed { value_type, .. }) => {
                    is_pod(module, *value_type, seen)?
                }
                TypeKind::Pack(Pack { members }) => {
                    for (_name, mtid) in members {
                        if !is_pod(module, *mtid, seen)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                // Non-POD kinds
                TypeKind::DynamicArray(_) | TypeKind::Sequence(_) | TypeKind::Variant(_) => false,
                // Alias points to underlying type
                TypeKind::Alias(inner) => {
                    // allocate a synthetic ID? Alias stores a Box<Type>; compare directly
                    match inner.kind {
                        TypeKind::Primitive(_) | TypeKind::Enum(_) | TypeKind::Bitfld(_) => true,
                        TypeKind::FixedArray(_) => true,
                        TypeKind::Pack(_) => true,
                        TypeKind::DynamicArray(_)
                        | TypeKind::Sequence(_)
                        | TypeKind::Variant(_) => false,
                        TypeKind::Alias(_) => false,
                    }
                }
            };
            seen.remove(&tid);
            Ok(res)
        }

        // iterate through all types and validate rules
        for (tid, t) in &self.types {
            match &t.kind {
                TypeKind::Pack(Pack { members }) => {
                    for (name, mtid) in members {
                        let mut seen = HashSet::new();
                        if !is_pod(self, *mtid, &mut seen)? {
                            let at = t.defined_at;

                            return Err(ModuleBuildError::NonPODInPack(name.to_string(), at));
                        }
                    }
                }
                TypeKind::Enum(Enum {
                    ty,
                    members,
                    default,
                }) => {
                    // ty is Primitive, must be integer
                    if !ty.is_integer() {
                        let check = self.name_and_span_for_tid(*tid)?;

                        return Err(ModuleBuildError::NonIntEnum(check.0.into(), check.1, *ty));
                    }
                    // We just checked for integer above...
                    let (min, max) = ty.int_bounds().expect("enum underlying bounds unavailable");

                    for (name, val) in members {
                        let v = *val as i128;
                        if v < min || v > max {
                            let at = t.defined_at;
                            return Err(ModuleBuildError::EnumValueOutOfRange {
                                name: name.to_string(),
                                at,
                                value: (*val).into(),
                                min,
                                max,
                            });
                        }
                    }
                    if let Some((name, val)) = default {
                        let v = *val as i128;
                        if v < min || v > max {
                            let at = t.defined_at;
                            return Err(ModuleBuildError::EnumValueOutOfRange {
                                name: name.to_string(),
                                at,
                                value: (*val).into(),
                                min,
                                max,
                            });
                        }
                    }
                }
                TypeKind::Bitfld(Bitfld { ty, members }) => {
                    // underlying must be primitive integer
                    let underlying = self.lookup(*ty)?;
                    let TypeKind::Primitive(p) = underlying.kind else {
                        let enum_type = self.name_and_span_for_tid(*tid)?;
                        let underlying = self.name_and_span_for_tid(*ty)?;

                        return Err(ModuleBuildError::NonIntBitfld(
                            enum_type.0.into(),
                            enum_type.1,
                            underlying.0.into(),
                        ));
                    };
                    if !p.is_integer() {
                        let enum_type = self.name_and_span_for_tid(*tid)?;
                        let underlying = self.name_and_span_for_tid(*ty)?;

                        return Err(ModuleBuildError::NonIntBitfld(
                            enum_type.0.into(),
                            enum_type.1,
                            underlying.0.into(),
                        ));
                    }
                    let width = p.bit_width().unwrap();
                    for (name, _ft, range) in members {
                        let end = *range.end();
                        if end >= width {
                            let at = t.defined_at;
                            return Err(ModuleBuildError::BitfldVOutOfRange {
                                name: name.into(),
                                at,
                                end,
                                width,
                            });
                        }
                    }
                }
                TypeKind::Variant(Variant {
                    ty,
                    members,
                    default,
                }) => {
                    // underlying must be primitive integer
                    if !ty.is_integer() {
                        let check = self.name_and_span_for_tid(*tid)?;

                        return Err(ModuleBuildError::NonIntVariant(
                            check.0.into(),
                            check.1,
                            *ty,
                        ));
                    }
                    // already checked its an int above.
                    // TODO: should probably collapse these checks into one
                    let (_min, max) = ty.int_bounds().unwrap();
                    for (val, _vt) in members {
                        let v = *val as i128;
                        if v > max {
                            let at = t.defined_at;
                            return Err(ModuleBuildError::VariantValueOutOfRange {
                                at,
                                value: v,
                                max,
                            });
                        }
                    }
                    if let Some((val, _vt)) = default {
                        let v = *val as i128;
                        if v > max {
                            let at = t.defined_at;
                            return Err(ModuleBuildError::VariantValueOutOfRange {
                                at,
                                value: v,
                                max,
                            });
                        }
                    }
                }
                TypeKind::FixedArray(ArrayKind::Fixed { value_type, .. }) => {
                    let mut seen = HashSet::new();
                    if !is_pod(self, *value_type, &mut seen)? {
                        let at = t.defined_at;
                        return Err(ModuleBuildError::FixedArrayRequiresPOD(at));
                    }
                }
                TypeKind::DynamicArray(ArrayKind::Dynamic {
                    size_type,
                    value_type,
                }) => {
                    // size type must be primitive integer
                    let st = self.lookup(*size_type)?;
                    let TypeKind::Primitive(p) = st.kind else {
                        let at = t.defined_at;
                        return Err(ModuleBuildError::ArraySizeTypeNotInteger {
                            n_m: "{N * M}",
                            at,
                        });
                    };
                    if !p.is_integer() {
                        let at = t.defined_at;
                        return Err(ModuleBuildError::ArraySizeTypeNotInteger {
                            n_m: "{N * M}",
                            at,
                        });
                    }
                    // value_type can be anything (dynamic or POD), no constraint here
                    let _ = value_type; // silence pattern warning if any
                }
                TypeKind::DynamicArray(ArrayKind::Fixed { count, .. }) => {
                    // {I * M}: I must be positive
                    if *count == 0 {
                        let at = t.defined_at;
                        return Err(ModuleBuildError::ArrayCountNotPositive { i_m: "{I * M}", at });
                    }
                }
                // Other kinds have no extra checks here
                _ => {}
            }
        }

        Ok(())
    }

    fn install_builtin_type(&mut self, mut name: &str, ty: Type) -> Result<TypeID> {
        name = name.trim();

        let id = *self.type_map.entry(name.to_string()).or_insert_with(|| {
            let ret = TypeID(self.last_tid);
            self.last_tid += 1;
            (ret, Span::Builtin)
        });

        assert!(self.types.insert(id.0, ty).is_none());

        Ok(id.0)
    }

    fn install_type_id(&mut self, mut name: &str, span: &Span) -> Result<TypeID> {
        name = name.trim();

        validate_type_name(name, span)?;

        if let Some((tid, _prev_decl_span)) = self.type_map.get(name) {
            // If the type id already has a definition, it's a duplicate
            if let Some(tdef) = self.types.get(tid) {
                let prev = match tdef.defined_at {
                    Span::Builtin => String::new(),
                    other => format!(" (previous definition at: {other})"),
                };
                return Err(ModuleBuildError::DuplicateTypeDefinition {
                    name: name.to_string(),
                    at: *span,
                    prev,
                });
            }
        }

        Ok(self
            .type_map
            .entry(name.to_string())
            .or_insert_with(|| {
                let ret = TypeID(self.last_tid);
                self.last_tid += 1;
                (ret, *span)
            })
            .0)
    }

    fn lookup_type_id(&mut self, mut name: &str, span: &Span) -> Result<TypeID> {
        name = name.trim();

        validate_type_name(name, span)?;

        Ok(self
            .type_map
            .entry(name.to_string())
            .or_insert_with(|| {
                let ret = TypeID(self.last_tid);
                self.last_tid += 1;
                (ret, *span)
            })
            .0)
    }

    pub fn name_and_span_for_tid(&self, t: TypeID) -> Result<(&String, Span)> {
        let check = self.type_map.iter().find(|x| x.1.0 == t);

        if let Some(check) = check {
            Ok((check.0, check.1.1))
        } else {
            Err(ModuleBuildError::UnknownType(t))
        }
    }

    pub fn lookup(&self, t: TypeID) -> Result<&Type> {
        self.types.get(&t).ok_or_else(|| {
            // expensive, but this is an error, so eh

            match self.name_and_span_for_tid(t) {
                Ok(check) => ModuleBuildError::BadLookup(check.0.clone(), check.1),
                Err(x) => x,
            }
        })
    }

    pub fn all_types(&self) -> &HashMap<TypeID, Type> {
        &self.types
    }

    // pub fn all_type_names(&self) -> &HashMap<String, TypeID> {
    //     &self.type_map
    // }

    fn add_builtins(&mut self) {
        let mut add_prim = |name: &str, p: Primitive| {
            self.install_builtin_type(
                name,
                Type {
                    defined_at: Span::Builtin,
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

fn validate_type_name(name: &str, span: &Span) -> Result<()> {
    if let Some(x) = name.chars().next() {
        if !x.is_alphabetic() {
            return Err(ModuleBuildError::InvalidTypeName {
                at: *span,
                reason: "types must start with alpha".to_string(),
            });
        }
    }

    for c in name.chars() {
        if c.is_whitespace() {
            return Err(ModuleBuildError::InvalidTypeName {
                at: *span,
                reason: "types must not contain whitespace".to_string(),
            });
        }
    }

    Ok(())
}

fn parse_decl_head(module: &mut PartialModule, reader: &mut TokenReader) -> Result<TypeID> {
    let tid = {
        let (ident, span) = reader.demand_identifier()?;

        module.install_type_id(&ident, &span)?
    };

    Ok(tid)
}

fn parse_decl_head_sub(module: &mut PartialModule, reader: &mut TokenReader) -> Result<TypeID> {
    reader.demand_symbol(Symbol::Colon)?;

    let (ident, span) = reader.demand_identifier()?;

    module.lookup_type_id(&ident, &span)
}

fn parse_body<F, G>(
    module: &mut PartialModule,
    reader: &mut TokenReader,
    mut f: F,
    mut def: G,
) -> Result<()>
where
    F: FnMut(&mut PartialModule, &mut TokenReader) -> Result<()>,
    G: FnMut(&mut PartialModule, &mut TokenReader) -> Result<()>,
{
    while let Some(sym) = reader.request_symbols(&[Symbol::Minus, Symbol::Equals]) {
        // consume the leading '-' or '=' before delegating
        reader.demand_symbol(sym)?;
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

fn parse_pack(module: &mut PartialModule, reader: &mut TokenReader) -> Result<()> {
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

            let mem_type = parse_inline_type(m, r)?;

            r.demand_newline()?;

            members.push((mem_name.0, mem_type));

            Ok(())
        },
        |_, r| {
            let at = r.current_span();
            Err(ModuleBuildError::UnexpectedDefault { kind: "pack", at })
        },
    )?;

    let at = at.union(&reader.current_span());

    module.types.insert(
        tid,
        Type {
            defined_at: at,
            kind: TypeKind::Pack(Pack { members }),
        },
    );

    Ok(())
}

fn parse_enum(module: &mut PartialModule, reader: &mut TokenReader) -> Result<()> {
    let at = reader.current_span();

    let tid = parse_decl_head(module, reader)?;

    let sub_tid = parse_decl_head_sub(module, reader)?;

    reader.demand_newline()?;

    let mut members = Vec::<_>::default();

    let mut default = Option::<_>::None;

    parse_body(
        module,
        reader,
        |_, r| {
            let mem_name = r.demand_identifier()?;

            r.demand_symbol(Symbol::Equals)?;

            let mem_value: i64 = match r.demand_number() {
                Ok(v) => v.0,
                Err(_) => return Err(ModuleBuildError::EnumValueNotInteger(r.current_span())),
            };

            r.demand_newline()?;

            members.push((mem_name.0, mem_value));

            Ok(())
        },
        |_, r| {
            let mem_name = r.demand_identifier()?;

            r.demand_symbol(Symbol::Equals)?;

            let mem_value: i64 = match r.demand_number() {
                Ok(v) => v.0,
                Err(_) => return Err(ModuleBuildError::EnumValueNotInteger(r.current_span())),
            };

            r.demand_newline()?;

            default = Some((mem_name.0, mem_value));

            Ok(())
        },
    )?;

    let at = at.union(&reader.current_span());

    let sub_type = module.lookup(sub_tid)?;

    match sub_type.kind {
        TypeKind::Primitive(p) => {
            if !p.is_integer() {
                let (ename, espan) = module.name_and_span_for_tid(tid)?;
                return Err(ModuleBuildError::NonIntEnum(ename.clone(), espan, p));
            }

            module.types.insert(
                tid,
                Type {
                    defined_at: at,
                    kind: TypeKind::Enum(Enum {
                        ty: p,
                        members,
                        default,
                    }),
                },
            );
        }
        _ => {
            let (ename, _espan) = module.name_and_span_for_tid(tid)?;
            let (bname, bspan) = module.name_and_span_for_tid(sub_tid)?;
            return Err(ModuleBuildError::NonPrimitiveEnumBase(
                ename.clone(),
                bname.clone(),
                bspan,
            ));
        }
    }

    Ok(())
}

fn parse_bits(module: &mut PartialModule, reader: &mut TokenReader) -> Result<()> {
    let at_start = reader.current_span();

    let tid = parse_decl_head(module, reader)?;
    let sub_tid = parse_decl_head_sub(module, reader)?;

    reader.demand_newline()?;

    let mut members: Vec<(String, TypeID, RangeInclusive<u32>)> = Vec::new();

    parse_body(
        module,
        reader,
        |m, r| {
            // parse bit range: start ['-' end]
            let token = r.demand_next()?;
            let (start, _) = match token.kind {
                tokens::TokenKind::Number(n) => (n as u32, token.span),
                _ => return Err(r.make_unexpected("number", token)),
            };
            let mut end: u32 = start;

            if matches!(r.request_symbols(&[Symbol::Minus]), Some(Symbol::Minus)) {
                r.demand_symbol(Symbol::Minus)?;
                let token = r.demand_next()?;
                end = match token.kind {
                    tokens::TokenKind::Number(n) => n as u32,
                    _ => return Err(r.make_unexpected("number", token)),
                };
            }

            let (field_name, field_span) = r.demand_identifier()?;
            r.demand_symbol(Symbol::Colon)?;
            let (ty_name, ty_span) = r.demand_identifier()?;
            r.demand_newline()?;

            let field_ty = m.lookup_type_id(&ty_name, &ty_span)?;

            // validate field type: only primitive or enum allowed
            let tdef = m.lookup(field_ty)?;
            match tdef.kind {
                TypeKind::Primitive(_) | TypeKind::Enum(_) => {}
                _ => {
                    return Err(ModuleBuildError::BitfieldMemberTypeInvalid {
                        field: field_name.clone(),
                        at: field_span,
                    });
                }
            }

            if start > end {
                return Err(ModuleBuildError::InvalidBitRange {
                    at: field_span,
                    start,
                    end,
                });
            }

            members.push((field_name, field_ty, start..=end));

            Ok(())
        },
        |_, r| {
            let at = r.current_span();
            Err(ModuleBuildError::UnexpectedDefault { kind: "bits", at })
        },
    )?;

    // validate underlying type is primitive integer
    let sub_type = module.lookup(sub_tid)?;
    match sub_type.kind {
        TypeKind::Primitive(p) => {
            if !p.is_integer() {
                let bit_type = module.name_and_span_for_tid(tid)?;
                let underlying = module.name_and_span_for_tid(sub_tid)?;
                return Err(ModuleBuildError::NonIntBitfld(
                    bit_type.0.clone(),
                    bit_type.1,
                    underlying.0.clone(),
                ));
            }
        }
        _ => {
            let bit_type = module.name_and_span_for_tid(tid)?;
            let underlying = module.name_and_span_for_tid(sub_tid)?;
            return Err(ModuleBuildError::NonIntBitfld(
                bit_type.0.clone(),
                bit_type.1,
                underlying.0.clone(),
            ));
        }
    }

    let at = at_start.union(&reader.current_span());
    module.types.insert(
        tid,
        Type {
            defined_at: at,
            kind: TypeKind::Bitfld(Bitfld {
                ty: sub_tid,
                members,
            }),
        },
    );

    Ok(())
}

fn parse_variant(module: &mut PartialModule, reader: &mut TokenReader) -> Result<()> {
    let at_start = reader.current_span();

    let tid = parse_decl_head(module, reader)?;
    let sub_tid = parse_decl_head_sub(module, reader)?;

    reader.demand_newline()?;

    let mut members: Vec<(u64, TypeID)> = Vec::new();
    let mut default: Option<(u64, TypeID)> = None;

    parse_body(
        module,
        reader,
        |m, r| {
            let (val_i64, _) = r.demand_number()?;
            if val_i64 < 0 {
                return Err(ModuleBuildError::VariantValueNegative(r.current_span()));
            }
            let val = val_i64 as u64;

            r.demand_fat_arrow()?;

            let (type_name, type_span) = r.demand_identifier()?;
            let ty = m.lookup_type_id(&type_name, &type_span)?;

            r.demand_newline()?;

            members.push((val, ty));
            Ok(())
        },
        |m, r| {
            let (val_i64, _) = r.demand_number()?;
            if val_i64 < 0 {
                return Err(ModuleBuildError::VariantValueNegative(r.current_span()));
            }
            let val = val_i64 as u64;

            r.demand_fat_arrow()?;

            let (type_name, type_span) = r.demand_identifier()?;
            let ty = m.lookup_type_id(&type_name, &type_span)?;

            r.demand_newline()?;

            default = Some((val, ty));
            Ok(())
        },
    )?;

    // validate discriminant type is primitive integer
    let sub_type = module.lookup(sub_tid)?;
    match sub_type.kind {
        TypeKind::Primitive(p) => {
            if !p.is_integer() {
                let (vname, vspan) = module.name_and_span_for_tid(tid)?;
                return Err(ModuleBuildError::NonIntVariant(vname.clone(), vspan, p));
            }

            let at = at_start.union(&reader.current_span());
            module.types.insert(
                tid,
                Type {
                    defined_at: at,
                    kind: TypeKind::Variant(Variant {
                        ty: p,
                        members,
                        default,
                    }),
                },
            );
        }
        _ => {
            let (vname, _vspan) = module.name_and_span_for_tid(tid)?;
            let (bname, bspan) = module.name_and_span_for_tid(sub_tid)?;
            return Err(ModuleBuildError::NonPrimitiveVariantBase(
                vname.clone(),
                bname.clone(),
                bspan,
            ));
        }
    }

    Ok(())
}

// helper to allocate anonymous types
fn alloc_anon_type(module: &mut PartialModule, ty: Type) -> TypeID {
    let id = TypeID(module.last_tid);
    module.last_tid += 1;
    module.types.insert(id, ty);
    id
}

fn parse_inline_type(module: &mut PartialModule, reader: &mut TokenReader) -> Result<TypeID> {
    // Decide based on next token: '{', '[', or identifier
    if reader.request_symbols(&[Symbol::LBrace]).is_some() {
        // { N * M } where N is type or number; treated as DynamicArray
        reader.demand_symbol(Symbol::LBrace)?;

        // First component: either number (fixed count) or identifier (size type)
        // Peek next token
        let token = reader.demand_next()?;
        match token.kind {
            tokens::TokenKind::Number(n) => {
                // { I * M }
                reader.demand_symbol(Symbol::Asterisk)?;
                let (val_name, val_span) = reader.demand_identifier()?;
                reader.demand_symbol(Symbol::RBrace)?;

                let value_type = module.lookup_type_id(&val_name, &val_span)?;
                let at = token.span.union(&reader.current_span());
                let tid = alloc_anon_type(
                    module,
                    Type {
                        defined_at: at,
                        kind: TypeKind::DynamicArray(ArrayKind::Fixed {
                            count: n,
                            value_type,
                        }),
                    },
                );
                Ok(tid)
            }
            tokens::TokenKind::Identifier(name) => {
                // { N * M }
                let size_type = module.lookup_type_id(&name, &token.span)?;
                reader.demand_symbol(Symbol::Asterisk)?;
                let (val_name, val_span) = reader.demand_identifier()?;
                reader.demand_symbol(Symbol::RBrace)?;

                let value_type = module.lookup_type_id(&val_name, &val_span)?;
                let at = token.span.union(&reader.current_span());
                let tid = alloc_anon_type(
                    module,
                    Type {
                        defined_at: at,
                        kind: TypeKind::DynamicArray(ArrayKind::Dynamic {
                            size_type,
                            value_type,
                        }),
                    },
                );
                Ok(tid)
            }
            other => Err(reader.make_unexpected(
                "number or identifier",
                Token {
                    kind: other,
                    span: token.span,
                },
            )),
        }
    } else if reader.request_symbols(&[Symbol::LBracket]).is_some() {
        // [ I * M ] -> FixedArray
        reader.demand_symbol(Symbol::LBracket)?;
        let (count_i64, sp) = reader.demand_number()?;
        if count_i64 < 0 {
            return Err(ModuleBuildError::ArrayCountNotPositive {
                i_m: "[I * M]",
                at: sp,
            });
        }
        let count = count_i64 as u64;
        reader.demand_symbol(Symbol::Asterisk)?;
        let (val_name, val_span) = reader.demand_identifier()?;
        reader.demand_symbol(Symbol::RBracket)?;

        let value_type = module.lookup_type_id(&val_name, &val_span)?;
        let at = reader.current_span();
        let tid = alloc_anon_type(
            module,
            Type {
                defined_at: at,
                kind: TypeKind::FixedArray(ArrayKind::Fixed { count, value_type }),
            },
        );
        Ok(tid)
    } else {
        // plain identifier
        let (name, span) = reader.demand_identifier()?;
        module.lookup_type_id(&name, &span)
    }
}

fn parse_seq(module: &mut PartialModule, reader: &mut TokenReader) -> Result<()> {
    let at_start = reader.current_span();
    let tid = parse_decl_head(module, reader)?;
    reader.demand_newline()?;

    let mut members: Vec<(String, TypeID)> = Vec::new();

    parse_body(
        module,
        reader,
        |m, r| {
            let (mem_name, _) = r.demand_identifier()?;
            r.demand_symbol(Symbol::Colon)?;

            let ty = parse_inline_type(m, r)?;

            r.demand_newline()?;

            members.push((mem_name, ty));
            Ok(())
        },
        |_, r| {
            let at = r.current_span();
            Err(ModuleBuildError::UnexpectedDefault {
                kind: "sequence",
                at,
            })
        },
    )?;

    let at = at_start.union(&reader.current_span());
    module.types.insert(
        tid,
        Type {
            defined_at: at,
            kind: TypeKind::Sequence(Sequence { members }),
        },
    );

    Ok(())
}
