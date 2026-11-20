//! C2-W-146: find lint pairs in the text runs the writer spells.
use super::node::{N, children, for_each_slot, id, is_inline_parent, text, ty};
use super::state::Writer;
use crate::pair_marks::{RULES, pairs};
use serde_json::Value;

type Place<'t> = Option<(N<'t>, usize)>;

fn spell<'t>(w: &Writer<'t>, node: N<'t>, source: &mut Vec<u8>, places: &mut Vec<Place<'t>>) {
    let value = text(node);
    let escapes = w.escapes_of(node);
    let bytes = value.as_bytes();
    for (at, &ch) in bytes.iter().enumerate() {
        let next = bytes.get(at + 1).copied();
        let doubled =
            ch == b'\\' && next.is_none_or(|n| n == b' ' || n == b'\n' || n.is_ascii_punctuation());
        if escapes.contains(&at) || doubled {
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

pub(super) fn asked<'t>(w: &Writer<'t>, group: &'t [Value]) -> Vec<(N<'t>, usize)> {
    let mut found = Vec::new();
    for node in group {
        walk(w, node, &mut found);
    }
    found.retain(|(node, at)| !w.escapes_of(node).contains(at));
    found.sort_unstable_by_key(|(node, at)| (id(node), *at));
    found.dedup_by_key(|(node, at)| (id(node), *at));
    found
}
