//! C2-W-146: find lint pairs in the text runs the writer spells.
use super::node::{N, children, for_each_slot, id, is_inline_parent, text, ty};
use super::state::Writer;
use crate::pair_marks::{RULES, pairs};
use crate::view::Arr;

type Place<'t> = Option<(N<'t>, usize)>;

fn spell<'t>(w: &Writer<'t>, node: N<'t>, source: &mut Vec<u8>, places: &mut Vec<Place<'t>>) {
    let value = text(node);
    let escapes = w.escapes_of(node);
    let bytes = value.as_bytes();
    let mut next_escape = 0;
    for (at, &ch) in bytes.iter().enumerate() {
        let next = bytes.get(at + 1).copied();
        let doubled =
            ch == b'\\' && next.is_none_or(|n| n == b' ' || n == b'\n' || n.is_ascii_punctuation());
        let asked = escapes.get(next_escape) == Some(&at);
        if asked {
            next_escape += 1;
        }
        if asked || doubled {
            source.push(b'\\');
            places.push(None);
        }
        source.push(ch);
        places.push(Some((node, at)));
    }
}

fn check<'t>(source: &[u8], places: &[Place<'t>], found: &mut Vec<(N<'t>, usize)>) {
    let Ok(source) = std::str::from_utf8(source) else {
        return;
    };
    for (mark, id) in RULES {
        for (start, _) in pairs(source, mark, id) {
            for at in start..start + mark.len() {
                if let Some(Some((node, offset))) = places.get(at) {
                    found.push((*node, *offset));
                }
            }
        }
    }
}

/// Whether a byte can start one of the marks of [`RULES`]; a run without one holds no pair.
pub(super) fn is_mark_byte(b: u8) -> bool {
    matches!(b, b'_' | b'~' | b'=' | b'*')
}

#[derive(Default)]
struct Run<'t> {
    source: Vec<u8>,
    places: Vec<Place<'t>>,
}

fn walk<'t>(w: &Writer<'t>, node: N<'t>, run: &mut Run<'t>, found: &mut Vec<(N<'t>, usize)>) {
    if is_inline_parent(node)
        && children(node)
            .iter()
            .any(|c| ty(c) == "text" && text(c).bytes().any(is_mark_byte))
    {
        run.source.clear();
        run.places.clear();
        for child in children(node) {
            match ty(child) {
                "text" => spell(w, child, &mut run.source, &mut run.places),
                "soft_break" => {
                    run.source.push(b'\n');
                    run.places.push(None);
                }
                _ => {
                    check(&run.source, &run.places, found);
                    run.source.clear();
                    run.places.clear();
                }
            }
        }
        check(&run.source, &run.places, found);
    }
    for_each_slot(node, true, &mut |kids, _, _| {
        for child in kids {
            walk(w, child, run, found);
        }
    });
}

/// Whether a text node below `value` holds a mark byte; a JSON tree without one holds no pair.
pub(super) fn json_has_mark_text(value: &serde_json::Value) -> bool {
    crate::view::json_any_text(value, &|t| t.bytes().any(is_mark_byte))
}

pub(super) fn asked<'t>(w: &Writer<'t>, group: Arr<'t>) -> Vec<(N<'t>, usize)> {
    if w.no_mark_text {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut run = Run::default();
    for node in group {
        walk(w, node, &mut run, &mut found);
    }
    found.retain(|(node, at)| w.escapes_of(node).binary_search(at).is_err());
    found.sort_unstable_by_key(|(node, at)| (id(node), *at));
    found.dedup_by_key(|(node, at)| (id(*node), *at));
    found
}

/// Merge the ordered requests for each text node with escapes already found by repair.
pub(super) fn apply(w: &mut Writer<'_>, asked: &[(N<'_>, usize)]) {
    for group in asked.chunk_by(|left, right| id(left.0) == id(right.0)) {
        let Some((node, _)) = group.first() else {
            continue;
        };
        let node_id = id(node);
        let old = w.sp.escapes.remove(&node_id).unwrap_or_default();
        let mut merged = Vec::with_capacity(old.len() + group.len());
        let mut before = old.into_iter().peekable();
        for (_, at) in group {
            while before.peek().is_some_and(|earlier| earlier < at) {
                merged.extend(before.next());
            }
            if before.peek() == Some(at) {
                before.next();
            }
            merged.push(*at);
        }
        merged.extend(before);
        w.sp.escapes.insert(node_id, merged);
    }
}
