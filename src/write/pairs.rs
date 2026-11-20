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

fn walk<'t>(w: &Writer<'t>, node: N<'t>, found: &mut Vec<(N<'t>, usize)>) {
    if is_inline_parent(node) {
        let mut source = Vec::new();
        let mut places = Vec::new();
        for child in children(node) {
            match ty(child) {
                "text" => spell(w, child, &mut source, &mut places),
                "soft_break" => {
                    source.push(b'\n');
                    places.push(None);
                }
                _ => {
                    check(&source, &places, found);
                    source.clear();
                    places.clear();
                }
            }
        }
        check(&source, &places, found);
    }
    for_each_slot(node, true, &mut |kids, _, _| {
        for child in kids {
            walk(w, child, found);
        }
    });
}

pub(super) fn asked<'t>(w: &Writer<'t>, group: Arr<'t>) -> Vec<(N<'t>, usize)> {
    let mut found = Vec::new();
    for node in group {
        walk(w, node, &mut found);
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
