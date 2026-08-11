//! Conservative three-way merge for the normative PART 12 exchange tree.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::Document;
use crate::ast_json::{from_json, parse_value, to_json, value_to_json, AstJsonError, Json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeConflictReason {
    BothChanged,
    DeleteEdit,
    ConcurrentSequenceEdit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    pub path: String,
    pub reason: MergeConflictReason,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MergeResult {
    Merged(Document),
    Conflicts(Vec<MergeConflict>),
}

fn clean(value: &Json, strip_metadata: bool) -> Json {
    match value {
        Json::Array(values) => Json::Array(
            values
                .iter()
                .map(|value| clean(value, strip_metadata))
                .collect(),
        ),
        Json::Object(values) => Json::Object(
            values
                .iter()
                .filter(|(key, _)| {
                    !strip_metadata || (key.as_str() != "pos" && key.as_str() != "srcByteLength")
                })
                .map(|(key, value)| {
                    (
                        key.clone(),
                        clean(value, strip_metadata && key != "keyValues"),
                    )
                })
                .collect(),
        ),
        value => value.clone(),
    }
}

fn strip_metadata(path: &str) -> bool {
    !path.split('/').any(|part| part == "keyValues")
}

fn same(a: Option<&Json>, b: Option<&Json>, path: &str) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => clean(a, strip_metadata(path)) == clean(b, strip_metadata(path)),
        (None, None) => true,
        _ => false,
    }
}

fn pointer(path: &str, key: impl ToString) -> String {
    let key = key.to_string().replace('~', "~0").replace('/', "~1");
    format!("{path}/{key}")
}

fn kind(value: &Json) -> String {
    match value {
        Json::Object(values) => match values.get("type") {
            Some(Json::String(kind)) => format!("node:{kind}"),
            _ => "object".into(),
        },
        Json::Array(_) => "array".into(),
        Json::Null => "null".into(),
        Json::Bool(_) => "boolean".into(),
        Json::Number(_) => "number".into(),
        Json::String(_) => "string".into(),
    }
}

fn identity_hint(value: &Json) -> Option<String> {
    let Json::Object(node) = value else {
        return None;
    };
    let Some(Json::String(kind)) = node.get("type") else {
        return None;
    };
    for field in ["label", "ref", "name"] {
        if let Some(Json::String(value)) = node.get(field) {
            return Some(format!("{kind}:{field}:{value}"));
        }
    }
    if let Some(Json::Object(attrs)) = node.get("attrs") {
        if let Some(Json::String(id)) = attrs.get("id") {
            return Some(format!("{kind}:attrs.id:{id}"));
        }
    }
    None
}

#[derive(Default)]
struct SideMatch {
    base_to_side: BTreeMap<usize, usize>,
    side_to_base: BTreeMap<usize, usize>,
    additions: Vec<usize>,
}

fn match_side(base: &[Json], side: &[Json], path: &str) -> SideMatch {
    let mut found = SideMatch::default();
    fn take(found: &mut SideMatch, bi: usize, si: usize) {
        found.base_to_side.insert(bi, si);
        found.side_to_base.insert(si, bi);
    }
    for (bi, value) in base.iter().enumerate() {
        if let Some(si) = side.iter().enumerate().find_map(|(si, candidate)| {
            (!found.side_to_base.contains_key(&si) && same(Some(value), Some(candidate), path))
                .then_some(si)
        }) {
            take(&mut found, bi, si);
        }
    }
    let remaining_base = |found: &SideMatch| {
        (0..base.len())
            .filter(|i| !found.base_to_side.contains_key(i))
            .collect::<Vec<_>>()
    };
    let remaining_side = |found: &SideMatch| {
        (0..side.len())
            .filter(|i| !found.side_to_base.contains_key(i))
            .collect::<Vec<_>>()
    };
    let mut base_hints = BTreeMap::<String, Vec<usize>>::new();
    for bi in remaining_base(&found) {
        if let Some(hint) = identity_hint(&base[bi]) {
            base_hints.entry(hint).or_default().push(bi);
        }
    }
    for bi in remaining_base(&found) {
        let Some(hint) = identity_hint(&base[bi]) else {
            continue;
        };
        let candidates = remaining_side(&found)
            .into_iter()
            .filter(|si| identity_hint(&side[*si]).as_ref() == Some(&hint))
            .collect::<Vec<_>>();
        if base_hints
            .get(&hint)
            .is_some_and(|indexes| indexes.len() == 1)
            && candidates.len() == 1
        {
            take(&mut found, bi, candidates[0]);
        }
    }
    let kinds = remaining_base(&found)
        .into_iter()
        .map(|i| kind(&base[i]))
        .collect::<BTreeSet<_>>();
    for value_kind in kinds {
        let bs = remaining_base(&found)
            .into_iter()
            .filter(|i| kind(&base[*i]) == value_kind)
            .collect::<Vec<_>>();
        let ss = remaining_side(&found)
            .into_iter()
            .filter(|i| kind(&side[*i]) == value_kind)
            .collect::<Vec<_>>();
        if bs.len() == 1 && ss.len() == 1 {
            take(&mut found, bs[0], ss[0]);
        }
    }
    let bs = remaining_base(&found);
    let ss = remaining_side(&found);
    if bs.len().saturating_mul(ss.len()) <= 1_000_000 {
        let mut table = vec![vec![0usize; ss.len() + 1]; bs.len() + 1];
        for i in (0..bs.len()).rev() {
            for j in (0..ss.len()).rev() {
                table[i][j] = if kind(&base[bs[i]]) == kind(&side[ss[j]]) {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < bs.len() && j < ss.len() {
            if kind(&base[bs[i]]) == kind(&side[ss[j]]) {
                take(&mut found, bs[i], ss[j]);
                i += 1;
                j += 1;
            } else if table[i + 1][j] >= table[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    } else {
        let mut cursor = 0;
        for bi in bs {
            while cursor < ss.len() && kind(&base[bi]) != kind(&side[ss[cursor]]) {
                cursor += 1;
            }
            if cursor == ss.len() {
                break;
            }
            take(&mut found, bi, ss[cursor]);
            cursor += 1;
        }
    }
    found.additions = (0..side.len())
        .filter(|i| !found.side_to_base.contains_key(i))
        .collect();
    found
}

fn anchor(index: usize, matched: &SideMatch, length: usize) -> (isize, isize) {
    let before = (0..index)
        .rev()
        .find_map(|i| matched.side_to_base.get(&i).copied())
        .map_or(-1, |v| v as isize);
    let after = (index + 1..length)
        .find_map(|i| matched.side_to_base.get(&i).copied())
        .map_or(-1, |v| v as isize);
    (before, after)
}

fn record_conflict(
    reason: MergeConflictReason,
    path: &str,
    base: Option<&Json>,
    ours: Option<&Json>,
    theirs: Option<&Json>,
    conflicts: &mut Vec<MergeConflict>,
) -> Option<Json> {
    conflicts.push(MergeConflict {
        path: path.into(),
        reason,
        base: base.map(value_to_json),
        ours: ours.map(value_to_json),
        theirs: theirs.map(value_to_json),
    });
    None
}

fn merge_sequence(
    base: &[Json],
    ours: &[Json],
    theirs: &[Json],
    path: &str,
    conflicts: &mut Vec<MergeConflict>,
) -> Option<Json> {
    let om = match_side(base, ours, path);
    let tm = match_side(base, theirs, path);
    let mut values = BTreeMap::<String, Json>::new();
    let mut omitted = BTreeSet::<String>::new();
    for (i, base_value) in base.iter().enumerate() {
        let (oi, ti) = (
            om.base_to_side.get(&i).copied(),
            tm.base_to_side.get(&i).copied(),
        );
        let token = format!("b{i}");
        match (oi, ti) {
            (None, None) => {
                omitted.insert(token);
            }
            (None, Some(ti)) if same(Some(base_value), Some(&theirs[ti]), path) => {
                omitted.insert(token);
            }
            (Some(oi), None) if same(Some(base_value), Some(&ours[oi]), path) => {
                omitted.insert(token);
            }
            (None, Some(ti)) => {
                record_conflict(
                    MergeConflictReason::DeleteEdit,
                    &pointer(path, i),
                    Some(base_value),
                    None,
                    Some(&theirs[ti]),
                    conflicts,
                );
                omitted.insert(token);
            }
            (Some(oi), None) => {
                record_conflict(
                    MergeConflictReason::DeleteEdit,
                    &pointer(path, i),
                    Some(base_value),
                    Some(&ours[oi]),
                    None,
                    conflicts,
                );
                omitted.insert(token);
            }
            (Some(oi), Some(ti)) => {
                if let Some(value) = merge_value(
                    Some(base_value),
                    Some(&ours[oi]),
                    Some(&theirs[ti]),
                    &pointer(path, i),
                    conflicts,
                ) {
                    values.insert(token, value);
                } else {
                    omitted.insert(token);
                }
            }
        }
    }
    let mut ours_add = BTreeMap::new();
    let mut theirs_add = BTreeMap::new();
    let mut used_theirs = BTreeSet::new();
    for &oi in &om.additions {
        let ours_hint = identity_hint(&ours[oi]);
        if tm.additions.iter().copied().any(|ti| {
            ours_hint.is_some()
                && identity_hint(&theirs[ti]) == ours_hint
                && anchor(oi, &om, ours.len()) == anchor(ti, &tm, theirs.len())
                && !same(Some(&ours[oi]), Some(&theirs[ti]), path)
        }) {
            return record_conflict(
                MergeConflictReason::ConcurrentSequenceEdit,
                path,
                Some(&Json::Array(base.to_vec())),
                Some(&Json::Array(ours.to_vec())),
                Some(&Json::Array(theirs.to_vec())),
                conflicts,
            );
        }
        let same_addition = tm.additions.iter().copied().find(|ti| {
            !used_theirs.contains(ti)
                && anchor(oi, &om, ours.len()) == anchor(*ti, &tm, theirs.len())
                && same(Some(&ours[oi]), Some(&theirs[*ti]), path)
        });
        let token = format!("o{oi}");
        ours_add.insert(oi, token.clone());
        values.insert(token.clone(), ours[oi].clone());
        if let Some(ti) = same_addition {
            theirs_add.insert(ti, token);
            used_theirs.insert(ti);
        }
    }
    for &ti in &tm.additions {
        if used_theirs.contains(&ti) {
            continue;
        }
        let token = format!("t{ti}");
        theirs_add.insert(ti, token.clone());
        values.insert(token, theirs[ti].clone());
    }
    let tokens_for =
        |side: &[Json], matched: &SideMatch, additions: &BTreeMap<usize, String>| -> Vec<String> {
            (0..side.len())
                .filter_map(|i| {
                    matched
                        .side_to_base
                        .get(&i)
                        .map(|bi| format!("b{bi}"))
                        .or_else(|| additions.get(&i).cloned())
                })
                .filter(|token| !omitted.contains(token))
                .collect()
        };
    let ot = tokens_for(ours, &om, &ours_add);
    let tt = tokens_for(theirs, &tm, &theirs_add);
    let surviving = (0..base.len())
        .map(|i| format!("b{i}"))
        .filter(|t| !omitted.contains(t))
        .collect::<Vec<_>>();
    let base_part = |tokens: &[String]| {
        tokens
            .iter()
            .filter(|t| t.starts_with('b'))
            .cloned()
            .collect::<Vec<_>>()
    };
    let (ours_moved, theirs_moved) = (base_part(&ot) != surviving, base_part(&tt) != surviving);
    let all = ot.iter().chain(&tt).cloned().collect::<BTreeSet<_>>();
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    let mut add_edges = |tokens: &[String], include_base: bool| {
        for pair in tokens.windows(2) {
            if !include_base && pair[0].starts_with('b') && pair[1].starts_with('b') {
                continue;
            }
            if pair[0] != pair[1] {
                edges
                    .entry(pair[0].clone())
                    .or_default()
                    .insert(pair[1].clone());
            }
        }
    };
    if !ours_moved && !theirs_moved {
        add_edges(&surviving, true);
        add_edges(&ot, false);
        add_edges(&tt, false);
    } else {
        add_edges(&ot, ours_moved);
        add_edges(&tt, theirs_moved);
    }
    let mut incoming = all
        .iter()
        .map(|t| (t.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for tos in edges.values() {
        for to in tos {
            *incoming.entry(to.clone()).or_default() += 1;
        }
    }
    let mut ready = all
        .iter()
        .filter(|t| incoming[*t] == 0)
        .cloned()
        .collect::<Vec<_>>();
    let mut order = Vec::new();
    while !ready.is_empty() {
        ready.sort();
        let token = ready.remove(0);
        order.push(token.clone());
        for to in edges.get(&token).into_iter().flatten() {
            let count = incoming.get_mut(to).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.push(to.clone());
            }
        }
    }
    if order.len() != all.len() {
        return record_conflict(
            MergeConflictReason::ConcurrentSequenceEdit,
            path,
            Some(&Json::Array(base.to_vec())),
            Some(&Json::Array(ours.to_vec())),
            Some(&Json::Array(theirs.to_vec())),
            conflicts,
        );
    }
    Some(Json::Array(
        order
            .into_iter()
            .filter_map(|token| values.remove(&token))
            .collect(),
    ))
}

fn merge_value(
    base: Option<&Json>,
    ours: Option<&Json>,
    theirs: Option<&Json>,
    path: &str,
    conflicts: &mut Vec<MergeConflict>,
) -> Option<Json> {
    if same(ours, theirs, path) {
        return ours.cloned();
    }
    if same(ours, base, path) {
        return theirs.cloned();
    }
    if same(theirs, base, path) {
        return ours.cloned();
    }
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return record_conflict(
            MergeConflictReason::DeleteEdit,
            path,
            base,
            ours,
            theirs,
            conflicts,
        );
    };
    if let (Some(Json::Array(base)), Json::Array(ours), Json::Array(theirs)) = (base, ours, theirs)
    {
        return merge_sequence(base, ours, theirs, path, conflicts);
    }
    if let (Some(Json::Object(base)), Json::Object(ours), Json::Object(theirs)) =
        (base, ours, theirs)
    {
        let keys = base
            .keys()
            .chain(ours.keys())
            .chain(theirs.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut out = BTreeMap::new();
        for key in keys {
            if strip_metadata(path) && (key == "pos" || key == "srcByteLength") {
                continue;
            }
            if let Some(value) = merge_value(
                base.get(&key),
                ours.get(&key),
                theirs.get(&key),
                &pointer(path, &key),
                conflicts,
            ) {
                out.insert(key, value);
            }
        }
        return Some(Json::Object(out));
    }
    record_conflict(
        MergeConflictReason::BothChanged,
        path,
        base,
        Some(ours),
        Some(theirs),
        conflicts,
    )
}

pub fn merge_ast(
    base: &Document,
    ours: &Document,
    theirs: &Document,
) -> Result<MergeResult, AstJsonError> {
    let base = parse_value(&to_json(base))?;
    let ours = parse_value(&to_json(ours))?;
    let theirs = parse_value(&to_json(theirs))?;
    let mut conflicts = Vec::new();
    let Some(mut merged) = merge_value(Some(&base), Some(&ours), Some(&theirs), "", &mut conflicts)
    else {
        return Ok(MergeResult::Conflicts(conflicts));
    };
    if !conflicts.is_empty() {
        return Ok(MergeResult::Conflicts(conflicts));
    }
    merged = clean(&merged, true);
    if let Json::Object(root) = &mut merged {
        root.insert("srcByteLength".into(), Json::Number(0));
    }
    Ok(MergeResult::Merged(from_json(&value_to_json(&merged))?))
}
