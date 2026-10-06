//! Conservative three-way merge for the normative PART 12 exchange tree.

use serde_json::Map;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ast::Document;
use crate::ast_fingerprint::Fingerprints;
use crate::ast_json::{from_json, parse_value, value_to_json, AstJsonError, Json};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeResolution {
    Base,
    Ours,
    Theirs,
    /// A JSON-encoded replacement value, which may target any PART 12 field.
    Value(String),
}

type Resolver<'a> = Option<&'a mut dyn FnMut(&MergeConflict) -> Option<MergeResolution>>;

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

struct MergePath {
    segments: Vec<String>,
    strip_metadata: bool,
}

fn same(
    a: Option<&Json>,
    b: Option<&Json>,
    path: &MergePath,
    fingerprints: &Fingerprints<'_>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            fingerprints.id(a, path.strip_metadata) == fingerprints.id(b, path.strip_metadata)
        }
        (None, None) => true,
        _ => false,
    }
}

fn pointer(path: &MergePath) -> String {
    let mut pointer = String::new();
    for segment in &path.segments {
        pointer.push('/');
        pointer.push_str(&segment.replace('~', "~0").replace('/', "~1"));
    }
    pointer
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

fn match_side(
    base: &[Json],
    side: &[Json],
    path: &MergePath,
    fingerprints: &Fingerprints<'_>,
) -> SideMatch {
    let mut found = SideMatch::default();
    fn take(found: &mut SideMatch, bi: usize, si: usize) {
        found.base_to_side.insert(bi, si);
        found.side_to_base.insert(si, bi);
    }
    let strip = path.strip_metadata;
    let mut exact = BTreeMap::<usize, VecDeque<usize>>::new();
    for (si, value) in side.iter().enumerate() {
        exact
            .entry(fingerprints.id(value, strip))
            .or_default()
            .push_back(si);
    }
    for (bi, value) in base.iter().enumerate() {
        let key = fingerprints.id(value, strip);
        if let Some(si) = exact.get_mut(&key).and_then(VecDeque::pop_front) {
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
    let mut side_hints = BTreeMap::<String, Vec<usize>>::new();
    for si in remaining_side(&found) {
        if let Some(hint) = identity_hint(&side[si]) {
            side_hints.entry(hint).or_default().push(si);
        }
    }
    for (hint, indexes) in &base_hints {
        if indexes.len() == 1 {
            if let Some(candidates) = side_hints.get(hint).filter(|items| items.len() == 1) {
                take(&mut found, indexes[0], candidates[0]);
            }
        }
    }
    let mut base_by_kind = BTreeMap::<String, Vec<usize>>::new();
    let mut side_by_kind = BTreeMap::<String, Vec<usize>>::new();
    for bi in remaining_base(&found) {
        base_by_kind.entry(kind(&base[bi])).or_default().push(bi);
    }
    for si in remaining_side(&found) {
        side_by_kind.entry(kind(&side[si])).or_default().push(si);
    }
    for (value_kind, bs) in base_by_kind {
        if bs.len() == 1 {
            if let Some(ss) = side_by_kind
                .get(&value_kind)
                .filter(|items| items.len() == 1)
            {
                take(&mut found, bs[0], ss[0]);
            }
        }
    }
    let bs = remaining_base(&found);
    let ss = remaining_side(&found);
    let base_kinds = bs.iter().map(|i| kind(&base[*i])).collect::<Vec<_>>();
    let side_kinds = ss.iter().map(|i| kind(&side[*i])).collect::<Vec<_>>();
    if bs.len().saturating_mul(ss.len()) <= 1_000_000 {
        record_pairings(bs.len().saturating_mul(ss.len()));
        let mut table = vec![vec![0usize; ss.len() + 1]; bs.len() + 1];
        for i in (0..bs.len()).rev() {
            for j in (0..ss.len()).rev() {
                table[i][j] = if base_kinds[i] == side_kinds[j] {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < bs.len() && j < ss.len() {
            if base_kinds[i] == side_kinds[j] {
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
        for (base_offset, bi) in bs.into_iter().enumerate() {
            while cursor < ss.len() && base_kinds[base_offset] != side_kinds[cursor] {
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

// Counts the candidate pairings the kind-LCS table allocates. Without a count
// the only way to observe the bound firing is a wall clock, which measures
// machine load rather than the merge (carve-rs#2190).
#[cfg(test)]
thread_local! {
    static PAIRINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_pairings(count: usize) {
    PAIRINGS.with(|cell| cell.set(cell.get() + count));
}

#[cfg(not(test))]
fn record_pairings(_count: usize) {}

fn anchor(index: usize, matched: &SideMatch) -> (isize, isize) {
    let before = matched
        .side_to_base
        .range(..index)
        .next_back()
        .map_or(-1, |(_, value)| *value as isize);
    let after = matched
        .side_to_base
        .range(index + 1..)
        .next()
        .map_or(-1, |(_, value)| *value as isize);
    (before, after)
}

fn record_conflict(
    reason: MergeConflictReason,
    path: &MergePath,
    base: Option<&Json>,
    ours: Option<&Json>,
    theirs: Option<&Json>,
    conflicts: &mut Vec<MergeConflict>,
    resolver: &mut Resolver<'_>,
) -> Result<Option<Json>, AstJsonError> {
    let conflict = MergeConflict {
        path: pointer(path),
        reason,
        base: base.map(value_to_json),
        ours: ours.map(value_to_json),
        theirs: theirs.map(value_to_json),
    };
    let resolution = resolver
        .as_deref_mut()
        .and_then(|resolve| resolve(&conflict));
    match resolution {
        Some(MergeResolution::Base) => Ok(base.cloned()),
        Some(MergeResolution::Ours) => Ok(ours.cloned()),
        Some(MergeResolution::Theirs) => Ok(theirs.cloned()),
        Some(MergeResolution::Value(value)) => Ok(Some(parse_value(&value)?)),
        None => {
            conflicts.push(conflict);
            Ok(None)
        }
    }
}

fn merge_sequence(
    base: &[Json],
    ours: &[Json],
    theirs: &[Json],
    path: &mut MergePath,
    fingerprints: &Fingerprints<'_>,
    conflicts: &mut Vec<MergeConflict>,
    resolver: &mut Resolver<'_>,
) -> Result<Option<Json>, AstJsonError> {
    let om = match_side(base, ours, path, fingerprints);
    let tm = match_side(base, theirs, path, fingerprints);
    let mut values = Map::<String, Json>::new();
    let mut omitted = BTreeSet::<String>::new();
    for (i, base_value) in base.iter().enumerate() {
        let (oi, ti) = (
            om.base_to_side.get(&i).copied(),
            tm.base_to_side.get(&i).copied(),
        );
        let token = format!("b{i}");
        path.segments.push(i.to_string());
        match (oi, ti) {
            (None, None) => {
                omitted.insert(token);
            }
            (None, Some(ti)) if same(Some(base_value), Some(&theirs[ti]), path, fingerprints) => {
                omitted.insert(token);
            }
            (Some(oi), None) if same(Some(base_value), Some(&ours[oi]), path, fingerprints) => {
                omitted.insert(token);
            }
            (None, Some(ti)) => {
                if let Some(value) = record_conflict(
                    MergeConflictReason::DeleteEdit,
                    path,
                    Some(base_value),
                    None,
                    Some(&theirs[ti]),
                    conflicts,
                    resolver,
                )? {
                    values.insert(token, value);
                } else {
                    omitted.insert(token);
                }
            }
            (Some(oi), None) => {
                if let Some(value) = record_conflict(
                    MergeConflictReason::DeleteEdit,
                    path,
                    Some(base_value),
                    Some(&ours[oi]),
                    None,
                    conflicts,
                    resolver,
                )? {
                    values.insert(token, value);
                } else {
                    omitted.insert(token);
                }
            }
            (Some(oi), Some(ti)) => {
                if let Some(value) = merge_value(
                    Some(base_value),
                    Some(&ours[oi]),
                    Some(&theirs[ti]),
                    path,
                    fingerprints,
                    conflicts,
                    resolver,
                )? {
                    values.insert(token, value);
                } else {
                    omitted.insert(token);
                }
            }
        }
        path.segments.pop();
    }
    let mut ours_add = BTreeMap::new();
    let mut theirs_add = BTreeMap::new();
    let mut used_theirs = BTreeSet::new();
    let mut additions_by_content = BTreeMap::<((isize, isize), usize), VecDeque<usize>>::new();
    let mut additions_by_hint = BTreeMap::<((isize, isize), String), (usize, bool)>::new();
    let strip = path.strip_metadata;
    for &ti in &tm.additions {
        let at = anchor(ti, &tm);
        let content = fingerprints.id(&theirs[ti], strip);
        if let Some(hint) = identity_hint(&theirs[ti]) {
            let entry = additions_by_hint
                .entry((at, hint))
                .or_insert((content, false));
            entry.1 |= entry.0 != content;
        }
        additions_by_content
            .entry((at, content))
            .or_default()
            .push_back(ti);
    }
    for &oi in &om.additions {
        let at = anchor(oi, &om);
        let content = fingerprints.id(&ours[oi], strip);
        let collision =
            identity_hint(&ours[oi]).and_then(|hint| additions_by_hint.get(&(at, hint)));
        if collision.is_some_and(|(first, differing)| *differing || *first != content) {
            return record_conflict(
                MergeConflictReason::ConcurrentSequenceEdit,
                path,
                Some(&Json::Array(base.to_vec())),
                Some(&Json::Array(ours.to_vec())),
                Some(&Json::Array(theirs.to_vec())),
                conflicts,
                resolver,
            );
        }
        let same_addition = additions_by_content
            .get_mut(&(at, content))
            .and_then(VecDeque::pop_front);
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
    let priority = |token: &String| {
        (
            token.as_bytes()[0],
            token[1..].parse::<usize>().unwrap(),
            token.clone(),
        )
    };
    let mut ready = all
        .iter()
        .filter(|t| incoming[*t] == 0)
        .map(priority)
        .collect::<BTreeSet<_>>();
    let mut order = Vec::new();
    while let Some((_, _, token)) = ready.pop_first() {
        order.push(token.clone());
        for to in edges.get(&token).into_iter().flatten() {
            let count = incoming.get_mut(to).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.insert(priority(to));
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
            resolver,
        );
    }
    Ok(Some(Json::Array(
        order
            .into_iter()
            .filter_map(|token| values.remove(&token))
            .collect(),
    )))
}

fn merge_value(
    base: Option<&Json>,
    ours: Option<&Json>,
    theirs: Option<&Json>,
    path: &mut MergePath,
    fingerprints: &Fingerprints<'_>,
    conflicts: &mut Vec<MergeConflict>,
    resolver: &mut Resolver<'_>,
) -> Result<Option<Json>, AstJsonError> {
    if same(ours, theirs, path, fingerprints) {
        return Ok(ours.cloned());
    }
    if same(ours, base, path, fingerprints) {
        return Ok(theirs.cloned());
    }
    if same(theirs, base, path, fingerprints) {
        return Ok(ours.cloned());
    }
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return record_conflict(
            MergeConflictReason::DeleteEdit,
            path,
            base,
            ours,
            theirs,
            conflicts,
            resolver,
        );
    };
    if let (Some(Json::Array(base)), Json::Array(ours), Json::Array(theirs)) = (base, ours, theirs)
    {
        return merge_sequence(base, ours, theirs, path, fingerprints, conflicts, resolver);
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
        let mut out = Map::new();
        for key in keys {
            if path.strip_metadata && (key == "pos" || key == "srcByteLength") {
                continue;
            }
            let previous_strip = path.strip_metadata;
            path.strip_metadata &= key != "keyValues";
            path.segments.push(key.clone());
            let merged = merge_value(
                base.get(&key),
                ours.get(&key),
                theirs.get(&key),
                path,
                fingerprints,
                conflicts,
                resolver,
            );
            path.strip_metadata = previous_strip;
            path.segments.pop();
            if let Some(value) = merged? {
                out.insert(key, value);
            }
        }
        return Ok(Some(Json::Object(out)));
    }
    record_conflict(
        MergeConflictReason::BothChanged,
        path,
        base,
        Some(ours),
        Some(theirs),
        conflicts,
        resolver,
    )
}

pub fn merge_ast(
    base: &Document,
    ours: &Document,
    theirs: &Document,
) -> Result<MergeResult, AstJsonError> {
    merge_ast_inner(base, ours, theirs, &mut None)
}

pub fn merge_ast_with_resolver<F>(
    base: &Document,
    ours: &Document,
    theirs: &Document,
    mut resolve: F,
) -> Result<MergeResult, AstJsonError>
where
    F: FnMut(&MergeConflict) -> Option<MergeResolution>,
{
    let mut resolver: Resolver<'_> = Some(&mut resolve);
    merge_ast_inner(base, ours, theirs, &mut resolver)
}

fn merge_ast_inner(
    base: &Document,
    ours: &Document,
    theirs: &Document,
    resolver: &mut Resolver<'_>,
) -> Result<MergeResult, AstJsonError> {
    let base = parse_value(&crate::ast_json::try_to_json(base)?)?;
    let ours = parse_value(&crate::ast_json::try_to_json(ours)?)?;
    let theirs = parse_value(&crate::ast_json::try_to_json(theirs)?)?;
    let mut fingerprints = Fingerprints::new(true, true);
    fingerprints.add(&base);
    fingerprints.add(&ours);
    fingerprints.add(&theirs);
    let mut conflicts = Vec::new();
    let mut path = MergePath {
        segments: Vec::new(),
        strip_metadata: true,
    };
    let Some(mut merged) = merge_value(
        Some(&base),
        Some(&ours),
        Some(&theirs),
        &mut path,
        &fingerprints,
        &mut conflicts,
        resolver,
    )?
    else {
        return Ok(MergeResult::Conflicts(conflicts));
    };
    if !conflicts.is_empty() {
        return Ok(MergeResult::Conflicts(conflicts));
    }
    merged = clean(&merged, true);
    if let Json::Object(root) = &mut merged {
        root.insert("srcByteLength".into(), Json::from(0));
    }
    Ok(MergeResult::Merged(from_json(&value_to_json(&merged))?))
}

#[cfg(test)]
mod bound_tests {
    use super::*;
    use crate::parse;

    fn pairings_for(width: usize) -> usize {
        let source = |prefix: &str| {
            (0..width)
                .map(|index| format!("{prefix}{index}"))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        PAIRINGS.with(|cell| cell.set(0));
        let result = merge_ast(
            &parse(&source("base-")),
            &parse(&source("ours-")),
            &parse(&source("theirs-")),
        )
        .unwrap();
        assert!(matches!(result, MergeResult::Conflicts(_)));
        PAIRINGS.with(std::cell::Cell::get)
    }

    #[test]
    fn a_narrow_ambiguous_sibling_list_is_paired_by_the_table() {
        // Two sides, so each one pairs 5x5 against the base.
        assert_eq!(pairings_for(5), 50);
    }

    #[test]
    fn a_wide_ambiguous_sibling_list_is_refused_by_the_bound() {
        assert_eq!(pairings_for(1001), 0);
    }
}
