use anyhow::{Context, anyhow, bail};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::ast::{Module, Type, TypeKind, TypeName};

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

        if imp.import_all {
            for def in &module_entries[module_idx].1.definitions {
                explicit_imports.insert(def.ident.clone());
            }
            continue;
        }

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
