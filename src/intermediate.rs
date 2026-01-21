use anyhow::{Context, anyhow, bail};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    io::BufRead,
    iter::Peekable,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct SourceCode(pub std::sync::Arc<String>);

impl SourceCode {
    /// Converts a 0-based line/column pair into a `SourceLocation` bound to this source.
    pub fn location(&self, line: usize, column: usize) -> SourceLocation {
        SourceLocation {
            source: self.clone(),
            position: Position { line, column },
        }
    }

    /// Converts a `Position` into a `SourceLocation` bound to this source.
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
    /// Constructs a source location from a source buffer and a position.
    pub fn new(source: SourceCode, position: Position) -> Self {
        Self { source, position }
    }
}

impl Display for SourceLocation {
    /// Formats a human-readable snippet (best-effort) for diagnostics.
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
    #[error("import declarations must appear before type definitions at {line}")]
    ImportAfterDeclaration { line: SourceLocation },
    #[error("invalid import at {line}: {reason}")]
    InvalidImport {
        line: SourceLocation,
        reason: String,
    },
    #[error("type name contains illegal character {reason}")]
    IllegalChar { line: SourceLocation, reason: char },
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct TypeName(std::sync::Arc<String>);

impl TypeName {
    #[allow(unused)]
    /// Returns the underlying string representation of the type name.
    fn as_str(&self) -> &str {
        &self.0
    }

    /// Creates a `TypeName` from a string slice (trimming whitespace).
    pub fn from_string(
        line: SourceLocation,
        x: impl AsRef<str>,
    ) -> Result<Self, IntermediateError> {
        let slice: &str = x.as_ref();
        if let Some(x) = slice.chars().find(|x| !is_legal_typename_char(*x)) {
            return Err(IntermediateError::IllegalChar { line, reason: x });
        }

        Ok(Self(std::sync::Arc::new(slice.trim().into())))
    }
}

impl Display for TypeName {
    /// Formats a type name as it appears in the DSL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn is_legal_typename_char(c: char) -> bool {
    matches!(c, 'a'..'z' | 'A'..'Z' | '0'..'9' | '_')
}

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub ty: TypeName,
    pub defined_at: SourceLocation,
}

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

        //dbg!(place, name);

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

/// Tagged union where the discriminant has a primitive integer type.
#[derive(Debug)]
pub struct Variant {
    pub ty: TypeName,
    pub members: Vec<VariantMember>,
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

    pub imports: Vec<Import>,

    pub definitions: Vec<Type>,
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
            //dbg!(&line);

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

            //dbg!(extra);

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

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub types: Vec<TypeName>,
    pub defined_at: SourceLocation,
}

/// Parses a `from <path> use {Type, ...}` import line.
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
    let inner = list
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .ok_or_else(|| IntermediateError::InvalidImport {
            line: code.location(line, list_start),
            reason: "expected `{...}` type list".into(),
        })?;

    let mut types = Vec::new();
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

    Ok(Import {
        path: path.into(),
        types,
        defined_at: code.location(line, 0),
    })
}

/// Loads a root module from disk, resolves imports, and returns a merged module.
pub fn load_module_with_imports(path: impl AsRef<Path>) -> anyhow::Result<Module> {
    let root_path = std::fs::canonicalize(path.as_ref())
        .with_context(|| format!("while resolving {}", path.as_ref().display()))?;

    let mut modules = HashMap::new();
    let mut visiting = HashSet::new();

    // Import modules used in this module
    load_modules_recursive(&root_path, &mut modules, &mut visiting)?;

    let mut module_entries: Vec<(PathBuf, Module)> = modules.into_iter().collect();

    module_entries.sort_by(|a, b| a.0.cmp(&b.0));

    let root_index = module_entries
        .iter()
        .position(|(path, _)| *path == root_path)
        .ok_or_else(|| anyhow!("root module missing after load"))?;

    if root_index != 0 {
        let root = module_entries.remove(root_index);
        module_entries.insert(0, root);
    }

    let root_name = module_entries[0].1.name.clone();
    let root_source = module_entries[0].1.source.clone();
    let root_imports = module_entries[0].1.imports.clone();

    let mut path_to_module = HashMap::new();
    for (idx, (path, _)) in module_entries.iter().enumerate() {
        path_to_module.insert(path.clone(), idx);
    }

    let mut index = HashMap::new();
    for (m_idx, (_, module)) in module_entries.iter().enumerate() {
        for (d_idx, def) in module.definitions.iter().enumerate() {
            if let Some((prev_m, prev_d)) = index.get(&def.ident) {
                // find previous definition

                let tmp: &(PathBuf, Module) = module_entries.get(*prev_m).unwrap();
                let tmp: &Module = &tmp.1;

                let prev: &Type = tmp.definitions.get(*prev_d).unwrap();

                bail!(
                    "duplicate type name {} defined at {} (previously defined at {})",
                    def.ident,
                    def.defined_at,
                    prev.defined_at
                );
            }
            index.insert(def.ident.clone(), (m_idx, d_idx));
        }
    }

    let mut explicit_imports = HashSet::new();
    let root_dir = root_path.parent().unwrap_or(Path::new("."));
    for imp in &root_imports {
        let resolved = resolve_import_path(root_dir, &imp.path);
        let resolved = std::fs::canonicalize(&resolved)
            .with_context(|| format!("while resolving import {}", imp.path))?;
        let Some(&module_idx) = path_to_module.get(&resolved) else {
            bail!(
                "import {} at {} could not be resolved",
                imp.path,
                imp.defined_at
            );
        };

        for ty in &imp.types {
            match index.get(ty) {
                Some((ty_mod_idx, _)) if *ty_mod_idx == module_idx => {
                    explicit_imports.insert(ty.clone());
                }
                Some((_, _)) => {
                    bail!(
                        "imported type {} at {} is defined in a different module",
                        ty,
                        imp.defined_at
                    );
                }
                None => {
                    bail!("imported type {} at {} does not exist", ty, imp.defined_at);
                }
            }
        }
    }

    let import_closure = dependency_closure(&explicit_imports, &module_entries, &index);

    let mut root_types = HashSet::new();
    for def in &module_entries[0].1.definitions {
        root_types.insert(def.ident.clone());
    }

    for def in &module_entries[0].1.definitions {
        for dep in direct_dependencies(def) {
            if root_types.contains(&dep) || import_closure.contains(&dep) || is_builtin(&dep) {
                continue;
            }
            bail!(
                "type {} used by {} at {} must be imported",
                dep,
                def.ident,
                def.defined_at
            );
        }
    }

    let mut allowed = root_types;
    allowed.extend(import_closure);

    let mut seen = HashSet::new();
    let mut definitions = Vec::new();
    for (_, mut module) in module_entries.into_iter() {
        for def in module.definitions.drain(..) {
            if allowed.contains(&def.ident) && seen.insert(def.ident.clone()) {
                definitions.push(def);
            }
        }
    }

    Ok(Module {
        name: root_name,
        source: root_source,
        imports: root_imports,
        definitions,
    })
}

fn load_modules_recursive(
    path: &Path,
    modules: &mut HashMap<PathBuf, Module>,
    visiting: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("while resolving {}", path.display()))?;

    if modules.contains_key(&canonical) {
        return Ok(());
    }

    if !visiting.insert(canonical.clone()) {
        bail!("cyclic import detected at {}", canonical.display());
    }

    let source = std::fs::read_to_string(&canonical)
        .with_context(|| format!("while reading {}", canonical.display()))?;
    let name = canonical
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("module")
        .to_string();
    let module = Module::from_string(name, source)
        .with_context(|| format!("while parsing {}", canonical.display()))?;

    let base_dir = canonical.parent().unwrap_or(Path::new("."));
    for imp in &module.imports {
        let import_path = resolve_import_path(base_dir, &imp.path);
        load_modules_recursive(&import_path, modules, visiting).with_context(|| {
            format!("while importing {} from {}", imp.path, canonical.display())
        })?;
    }

    visiting.remove(&canonical);
    modules.insert(canonical, module);
    Ok(())
}

fn resolve_import_path(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn dependency_closure(
    seeds: &HashSet<TypeName>,
    modules: &[(PathBuf, Module)],
    index: &HashMap<TypeName, (usize, usize)>,
) -> HashSet<TypeName> {
    let mut out = seeds.clone();
    let mut stack: Vec<TypeName> = seeds.iter().cloned().collect();

    while let Some(name) = stack.pop() {
        let Some((m_idx, d_idx)) = index.get(&name) else {
            continue;
        };
        let ty = &modules[*m_idx].1.definitions[*d_idx];
        for dep in direct_dependencies(ty) {
            if out.insert(dep.clone()) {
                stack.push(dep);
            }
        }
    }

    out
}

fn direct_dependencies(ty: &Type) -> Vec<TypeName> {
    match &ty.kind {
        TypeKind::Alias(alias) => vec![alias.other.clone()],
        TypeKind::Pack(pack) => pack.members.iter().map(|m| m.ty.clone()).collect(),
        TypeKind::Enum(enm) => vec![enm.ty.clone()],
        TypeKind::Bitfld(bitfld) => {
            let mut deps: Vec<TypeName> = Vec::with_capacity(bitfld.members.len() + 1);
            deps.push(bitfld.ty.clone());
            deps.extend(bitfld.members.iter().map(|m| m.ty.clone()));
            deps
        }
        TypeKind::Variant(variant) => {
            let mut deps: Vec<TypeName> = Vec::with_capacity(variant.members.len() + 1);
            deps.push(variant.ty.clone());
            deps.extend(variant.members.iter().map(|m| m.ty.clone()));
            deps
        }
        TypeKind::Sequence(sequence) => sequence.members.iter().map(|m| m.ty.clone()).collect(),
        TypeKind::DynamicArray(dynamic_array) => {
            vec![
                dynamic_array.size_type.clone(),
                dynamic_array.value_type.clone(),
            ]
        }
        TypeKind::FixedArray(fixed_array) => vec![fixed_array.value_type.clone()],
    }
}

fn is_builtin(name: &TypeName) -> bool {
    matches!(
        name.as_str(),
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "void"
    )
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
        if line.starts_with("-") || line.starts_with(">") {
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

        //dbg!(&members);

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
        let source = include_str!("../assets/example.jaw");

        let module = Module::from_string("file".into(), source.into()).expect("parse module");

        let m: HashMap<_, _> = module
            .definitions
            .into_iter()
            .map(|x| (x.ident.clone(), x))
            .collect();

        assert_eq!(m.len(), 15);

        let quick_typename = |name: &str| -> TypeName {
            TypeName::from_string(
                SourceLocation {
                    source: SourceCode(std::sync::Arc::new(module.source.clone())),
                    position: Position { line: 0, column: 0 },
                },
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
        assert_eq!(
            imp.types.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
            vec!["Thing", "OtherThing"]
        );
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
}
