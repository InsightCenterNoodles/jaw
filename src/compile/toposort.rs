use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
};

use super::{Type, TypeID, TypeKind};

/// Returns all layout-affecting `TypeID` dependencies referenced by a type.
///
/// Dependencies that only introduce indirection (e.g., dynamic array element types)
/// are intentionally excluded so recursive definitions like `Any -> AnyArray -> Any`
/// can be represented safely.
fn layout_dependencies(ty: &Type) -> Vec<TypeID> {
    match &ty.kind {
        TypeKind::Pack(pack) => pack.members.iter().map(|m| m.ty).collect(),
        TypeKind::Enum(enm) => vec![enm.underlying],
        TypeKind::Bitfld(bitfld) => {
            let mut deps: Vec<TypeID> = Vec::with_capacity(bitfld.members.len() + 1);
            deps.push(bitfld.underlying);
            deps.extend(bitfld.members.iter().map(|m| m.underlying));
            deps
        }
        TypeKind::Variant(variant) => {
            let mut deps: Vec<TypeID> = Vec::with_capacity(variant.members.len() + 1);
            deps.push(variant.discriminant);
            deps.extend(variant.members.iter().map(|m| m.ty));
            deps
        }
        TypeKind::Sequence(sequence) => sequence.members.iter().map(|m| m.ty).collect(),
        TypeKind::DynamicArray(dynamic_array) => vec![dynamic_array.size_type],
        TypeKind::FixedArray(fixed_array) => vec![fixed_array.value_type],
        TypeKind::Const(c) => vec![c.ty],
        TypeKind::Primitive(_) | TypeKind::Void => Vec::new(),
    }
}

/// Finds an example dependency cycle, if one exists.
fn find_cycle(defs: &HashMap<TypeID, Type>) -> Option<Vec<TypeID>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Visiting,
        Visited,
    }

    /// DFS helper used to reconstruct a cycle path.
    fn dfs(
        node: TypeID,
        defs: &HashMap<TypeID, Type>,
        state: &mut HashMap<TypeID, State>,
        stack: &mut Vec<TypeID>,
    ) -> Option<Vec<TypeID>> {
        state.insert(node, State::Visiting);
        stack.push(node);

        let ty = defs.get(&node)?;
        for dep in layout_dependencies(ty) {
            if !defs.contains_key(&dep) {
                continue;
            }
            if matches!(state.get(&dep), Some(State::Visiting)) {
                let start = stack.iter().position(|&x| x == dep).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(dep);
                return Some(cycle);
            }
            if !matches!(state.get(&dep), Some(State::Visited)) {
                if let Some(found) = dfs(dep, defs, state, stack) {
                    return Some(found);
                }
            }
        }

        state.insert(node, State::Visited);
        stack.pop();
        None
    }

    let mut state = HashMap::new();
    let mut stack = Vec::new();

    for &id in defs.keys() {
        if state.contains_key(&id) {
            continue;
        }
        if let Some(cycle) = dfs(id, defs, &mut state, &mut stack) {
            return Some(cycle);
        }
    }

    None
}

/// Produces a stable topological ordering over all definitions.
pub(super) fn toposort(defs: &HashMap<TypeID, Type>) -> anyhow::Result<Vec<TypeID>> {
    let mut indegree: HashMap<TypeID, usize> = HashMap::new();
    let mut dependents: HashMap<TypeID, Vec<TypeID>> = HashMap::new();

    let mut ids: Vec<_> = defs.keys().copied().collect();
    ids.sort();

    for &id in &ids {
        indegree.insert(id, 0);
        dependents.insert(id, Vec::new());
    }

    for &id in &ids {
        let ty = defs
            .get(&id)
            .expect("type id inserted into indegree without definition");

        let mut seen = HashSet::new();
        for dep in layout_dependencies(ty) {
            if !defs.contains_key(&dep) {
                continue;
            }
            if seen.insert(dep) {
                dependents.entry(dep).or_default().push(id);
                *indegree
                    .get_mut(&id)
                    .expect("indegree missing for previously inserted id") += 1;
            }
        }
    }

    for deps in dependents.values_mut() {
        deps.sort();
    }

    // Use a min-heap on TypeID to make the traversal deterministic across runs.
    let mut queue: BinaryHeap<Reverse<TypeID>> = indegree
        .iter()
        .filter_map(|(&id, &deg)| if deg == 0 { Some(Reverse(id)) } else { None })
        .collect();
    let mut order: Vec<TypeID> = Vec::with_capacity(indegree.len());

    while let Some(Reverse(id)) = queue.pop() {
        order.push(id);
        if let Some(nexts) = dependents.get(&id) {
            for &n in nexts {
                if let Some(d) = indegree.get_mut(&n) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push(Reverse(n));
                    }
                }
            }
        }
    }

    if order.len() < indegree.len() {
        if let Some(cycle) = find_cycle(defs) {
            let cycle_names: Vec<_> = cycle
                .into_iter()
                .filter_map(|id| defs.get(&id).map(|ty| ty.ident.to_string()))
                .collect();
            anyhow::bail!(
                "cyclic type definitions detected: {}",
                cycle_names.join(" -> ")
            );
        } else {
            anyhow::bail!("cyclic type definitions detected");
        }
    }

    Ok(order)
}
