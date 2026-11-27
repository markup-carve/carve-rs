//! C2-W-091: a group of one paragraph whose authored text, tree and per-node state equal an earlier group's of
//! the same write reads the same texts in its rounds and decides the same escapes, so it takes that group's result
//! instead of running them again. Its repair work still counts toward the budget of C2-W-133.

use super::node::{N, id, span, trees_equal, ty};
use super::state::{Below, Facts, GroupCertificate, Loss, Writer};
use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use std::collections::HashMap;

/// The per-node state of a group, by the place of each node in the group's walk (`group_node_ids`).
#[derive(Clone, PartialEq, Default)]
pub struct Snapshot {
    attempts: Vec<(usize, usize, Mapped)>,
    link_escapes: Placed,
    escapes: Vec<(usize, Vec<usize>)>,
    lost: Vec<usize>,
    lost_keys: Vec<(usize, Vec<String>)>,
    forms: Vec<(usize, u32)>,
    round_forms: Vec<usize>,
    glued: Vec<usize>,
    colon_runs: Vec<(usize, usize)>,
    kinds_below: Vec<(usize, u8)>,
    below_list: Vec<(usize, Below)>,
    indented: Vec<(usize, bool)>,
    read_comments: Vec<usize>,
    top_blocks: Vec<usize>,
    open_above: Vec<(usize, usize)>,
    link_brackets: Vec<(usize, (usize, usize))>,
    case_eight_links: Vec<usize>,
    unknown_alts: Vec<usize>,
    collapsed_reads: Vec<(usize, Vec<(String, bool)>)>,
    facts: Vec<(usize, Facts)>,
    wrap_points: Vec<(usize, Vec<usize>)>,
    wrap_starts: Vec<usize>,
}

fn entries<T: Clone>(map: &AddrMap<usize, T>, ids: &[usize]) -> Vec<(usize, T)> {
    if map.is_empty() {
        return Vec::new();
    }
    ids.iter()
        .enumerate()
        .filter_map(|(i, n)| map.get(n).map(|v| (i, v.clone())))
        .collect()
}

fn members(set: &AddrSet<usize>, ids: &[usize]) -> Vec<usize> {
    if set.is_empty() {
        return Vec::new();
    }
    ids.iter()
        .enumerate()
        .filter(|(_, n)| set.contains(*n))
        .map(|(i, _)| i)
        .collect()
}

fn put<T>(map: &mut AddrMap<usize, T>, ids: &[usize], from: Vec<(usize, T)>) {
    if !map.is_empty() {
        for n in ids {
            map.remove(n);
        }
    }
    for (i, v) in from {
        if let Some(&n) = ids.get(i) {
            map.insert(n, v);
        }
    }
}

fn put_set(set: &mut AddrSet<usize>, ids: &[usize], from: Vec<usize>) {
    if !set.is_empty() {
        for n in ids {
            set.remove(n);
        }
    }
    set.extend(from.into_iter().filter_map(|i| ids.get(i).copied()));
}

/// The nodes of `group` in the order of `group_node_ids`, so equal groups map node for node.
fn group_nodes(group: Arr<'_>) -> Vec<N<'_>> {
    fn walk<'t>(v: V<'t>, out: &mut Vec<N<'t>>) {
        if let V::Json(value) = v {
            walk_json(value, out);
        } else if let V::Pos(_) | V::Attrs(_) | V::Marker(_) | V::Task(_) = v {
            out.push(v);
        } else if let Some(items) = v.as_array() {
            for item in items {
                walk(item, out);
            }
        } else if v.is_object() {
            out.push(v);
            v.each(&mut |_, value| walk(value, out));
        }
    }
    fn walk_json<'t>(value: &'t serde_json::Value, out: &mut Vec<N<'t>>) {
        match value {
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk_json(item, out)),
            serde_json::Value::Object(map) => {
                out.push(V::Json(value));
                map.values().for_each(|item| walk_json(item, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for node in group {
        walk(node, &mut out);
    }
    out
}

/// Escapes that name nodes, each node by its place in the walk.
type Mapped = Vec<(usize, usize)>;

/// [`Mapped`] escapes by the place of the node they belong to.
type Placed = Vec<(usize, Mapped)>;

fn placed(map: &AddrMap<usize, Vec<(N<'_>, usize)>>, ids: &[usize]) -> Option<Placed> {
    if map.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for (i, n) in ids.iter().enumerate() {
        let Some(list) = map.get(n) else { continue };
        let mut mapped = Vec::with_capacity(list.len());
        for (m, at) in list {
            mapped.push((ids.iter().position(|&k| k == id(*m))?, *at));
        }
        out.push((i, mapped));
    }
    Some(out)
}

fn unplace<'t>(from: &[(usize, usize)], nodes: &[N<'t>]) -> Vec<(N<'t>, usize)> {
    from.iter()
        .filter_map(|&(i, at)| nodes.get(i).map(|&n| (n, at)))
        .collect()
}

/// The state of the nodes `ids`, `None` where one of them holds state that names other nodes, which a later
/// group could not take over.
fn snapshot(w: &Writer<'_>, ids: &[usize]) -> Option<Snapshot> {
    let attempts = if w.sp.attempts.is_empty() {
        Vec::new()
    } else {
        let mut out = Vec::new();
        for (i, n) in ids.iter().enumerate() {
            let Some(a) = w.sp.attempts.get(n) else {
                continue;
            };
            let mut mapped = Vec::with_capacity(a.escapes.len());
            for (m, at) in &a.escapes {
                mapped.push((ids.iter().position(|&k| k == id(*m))?, *at));
            }
            out.push((i, a.step, mapped));
        }
        out
    };
    Some(Snapshot {
        attempts,
        link_escapes: placed(&w.link_escapes, ids)?,
        escapes: entries(&w.sp.escapes, ids),
        lost: members(&w.sp.lost, ids),
        lost_keys: entries(&w.sp.lost_keys, ids),
        forms: entries(&w.forms, ids),
        round_forms: members(&w.round_forms, ids),
        glued: members(&w.glued, ids),
        colon_runs: entries(&w.colon_runs, ids),
        kinds_below: entries(&w.kinds_below, ids),
        below_list: entries(&w.below_list, ids),
        indented: entries(&w.indented, ids),
        read_comments: members(&w.read_comments, ids),
        top_blocks: members(&w.top_blocks, ids),
        open_above: entries(&w.open_above, ids),
        link_brackets: entries(&w.link_brackets, ids),
        case_eight_links: members(&w.case_eight_links, ids),
        unknown_alts: members(&w.unknown_alts, ids),
        collapsed_reads: entries(&w.collapsed_reads, ids),
        facts: w
            .facts
            .as_ref()
            .map(|f| entries(f, ids))
            .unwrap_or_default(),
        wrap_points: w
            .wrap_points
            .as_ref()
            .map(|f| entries(f, ids))
            .unwrap_or_default(),
        wrap_starts: w
            .wrap_starts
            .as_ref()
            .map(|f| members(f, ids))
            .unwrap_or_default(),
    })
}

fn restore<'t>(w: &mut Writer<'t>, ids: &[usize], nodes: &[N<'t>], s: Snapshot) {
    put(
        &mut w.sp.attempts,
        ids,
        s.attempts
            .iter()
            .map(|(i, step, list)| {
                let attempt = super::state::Attempt {
                    step: *step,
                    escapes: unplace(list, nodes),
                };
                (*i, attempt)
            })
            .collect(),
    );
    put(
        &mut w.link_escapes,
        ids,
        s.link_escapes
            .iter()
            .map(|(i, list)| (*i, unplace(list, nodes)))
            .collect(),
    );
    put(&mut w.sp.escapes, ids, s.escapes);
    put_set(&mut w.sp.lost, ids, s.lost);
    put(&mut w.sp.lost_keys, ids, s.lost_keys);
    put(&mut w.forms, ids, s.forms);
    put_set(&mut w.round_forms, ids, s.round_forms);
    put_set(&mut w.glued, ids, s.glued);
    put(&mut w.colon_runs, ids, s.colon_runs);
    put(&mut w.kinds_below, ids, s.kinds_below);
    put(&mut w.below_list, ids, s.below_list);
    put(&mut w.indented, ids, s.indented);
    put_set(&mut w.read_comments, ids, s.read_comments);
    put_set(&mut w.top_blocks, ids, s.top_blocks);
    put(&mut w.open_above, ids, s.open_above);
    put(&mut w.link_brackets, ids, s.link_brackets);
    put_set(&mut w.case_eight_links, ids, s.case_eight_links);
    put_set(&mut w.unknown_alts, ids, s.unknown_alts);
    put(&mut w.collapsed_reads, ids, s.collapsed_reads);
    if let Some(f) = w.facts.as_mut() {
        put(f, ids, s.facts);
    }
    if let Some(f) = w.wrap_points.as_mut() {
        put(f, ids, s.wrap_points);
    }
    if let Some(f) = w.wrap_starts.as_mut() {
        put_set(f, ids, s.wrap_starts);
    }
}

/// What one group of the memo left behind.
struct Taken<'t> {
    node: N<'t>,
    before: Snapshot,
    after: Snapshot,
    lines: Vec<String>,
    /// The losses, each with the place of its node in the group's walk.
    losses: Vec<(Loss, Option<usize>)>,
    work: (usize, usize),
    certificate: GroupCertificate,
    links: (bool, bool),
}

#[derive(Default)]
pub struct GroupMemo<'t> {
    taken: HashMap<String, Vec<Taken<'t>>>,
}

/// A group that may be taken from or kept in the memo, with its state before the rounds.
pub struct Pending<'t> {
    key: String,
    node: N<'t>,
    group: Arr<'t>,
    ids: std::rc::Rc<Vec<usize>>,
    before: Snapshot,
    work: (usize, usize),
    losses: usize,
}

/// The group of one paragraph at `from`, where the memo can serve it: not the first group, whose spelling
/// depends on the blank line above, and with a source to tell equal authored texts apart.
pub fn pending<'t>(
    w: &Writer<'t>,
    nodes: Arr<'t>,
    from: usize,
    first: bool,
) -> Option<Pending<'t>> {
    if first {
        return None;
    }
    let node = nodes.get(from)?;
    if ty(node) != "paragraph" {
        return None;
    }
    let (start, end) = span(node)?;
    let key = w.authored.as_deref()?.get(start..end)?.to_string();
    let group = nodes.slice(from, from + 1)?;
    let ids = super::group_node_ids(w, group);
    let before = snapshot(w, &ids)?;
    Some(Pending {
        key,
        node,
        group,
        ids,
        before,
        work: (w.work_chars, w.parses),
        losses: w.sp.losses.len(),
    })
}

impl<'t> GroupMemo<'t> {
    /// Writes the group of `p` as the equal earlier group was written, where its work fits the budget.
    pub fn take(&self, w: &mut Writer<'t>, p: &Pending<'t>) -> bool {
        let Some(found) = self.taken.get(&p.key).and_then(|list| {
            list.iter()
                .find(|t| t.before == p.before && trees_equal(t.node, p.node))
        }) else {
            return false;
        };
        let limit = w.work_limit.min(w.work_cap);
        if w.work_chars.saturating_add(found.work.0) > limit {
            return false;
        }
        w.work_chars += found.work.0;
        w.parses += found.work.1;
        let nodes = group_nodes(p.group);
        if nodes.len() != p.ids.len() {
            return false;
        }
        restore(w, &p.ids, &nodes, found.after.clone());
        for (loss, at) in &found.losses {
            let mut loss = loss.clone();
            loss.node = at.and_then(|i| p.ids.get(i).copied());
            w.sp.losses.push(loss);
        }
        w.lines.clone_from(&found.lines);
        w.group_certificate = found.certificate;
        (w.link_bracket_fallback, w.saw_link) = found.links;
        true
    }

    /// Keeps the result of the group of `p`, written in one group of one node, where nothing in it names a node
    /// another group could not map and its rounds stayed inside the budget.
    pub fn keep(&mut self, w: &Writer<'t>, p: Pending<'t>, end: usize, from: usize) {
        if end != from + 1
            || w.work_chars > w.work_limit.min(w.work_cap)
            || !w.capped_runs.is_empty()
        {
            return;
        }
        let Some(after) = snapshot(w, &p.ids) else {
            return;
        };
        let mut losses = Vec::new();
        for loss in w.sp.losses.get(p.losses..).unwrap_or_default() {
            let at = match loss.node {
                Some(n) => match p.ids.iter().position(|&m| m == n) {
                    Some(i) => Some(i),
                    None => return,
                },
                None => None,
            };
            losses.push((loss.clone(), at));
        }
        let list = self.taken.entry(p.key).or_default();
        if list.len() >= MAX_TAKEN_PER_TEXT
            || list
                .iter()
                .any(|t| t.before == p.before && trees_equal(t.node, p.node))
        {
            return;
        }
        list.push(Taken {
            node: p.node,
            before: p.before,
            after,
            lines: w.lines.clone(),
            losses,
            work: (w.work_chars - p.work.0, w.parses - p.work.1),
            certificate: w.group_certificate,
            links: (w.link_bracket_fallback, w.saw_link),
        });
    }
}

/// Distinct states of one authored text the memo keeps.
const MAX_TAKEN_PER_TEXT: usize = 4;

#[cfg(test)]
mod tests {
    use super::super::{PAIR_SCANS, Pool, Settings, format_view};
    use crate::view::V;

    /// The output of `fmt` on `src` and the groups that ran their own rounds.
    fn format(src: &str) -> (String, usize) {
        let arena = crate::Arena::for_input(src);
        let doc =
            crate::parse_resolved_in(src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let before = PAIR_SCANS.with(std::cell::Cell::get);
        let (out, _) = format_view(V::Doc(&doc), &pool, Some(src), &Settings::default());
        (out, PAIR_SCANS.with(std::cell::Cell::get) - before)
    }

    /// carve-rs#4914: equal paragraphs run their rounds once, and write the bytes each would alone.
    #[test]
    fn equal_paragraphs_take_the_earlier_rounds() {
        for shape in [r"\[^[\`[`[[^]\[]][]\[", "[a *[a][]*[x]: /u", "x<a:b>=}"] {
            let (one, _) = format(&format!("{shape}\n"));
            let (few, few_runs) = format(&format!("{shape}\n\n").repeat(4));
            let (many, many_runs) = format(&format!("{shape}\n\n").repeat(64));
            assert_eq!(few, [one.trim_end(); 4].join("\n\n") + "\n", "{shape}");
            assert_eq!(
                many,
                vec![one.trim_end(); 64].join("\n\n") + "\n",
                "{shape}"
            );
            assert_eq!(few_runs, many_runs, "{shape}");
        }
    }

    /// An equal text whose tree differs runs its own rounds.
    #[test]
    fn a_paragraph_with_other_attributes_runs_its_rounds() {
        let src = "a <b> [c]\n\n{.c}\na <b> [c]\n\na <b> [c]\n\n{#d}\na <b> [c]\n";
        assert_eq!(format(src).0, src);
    }
}
