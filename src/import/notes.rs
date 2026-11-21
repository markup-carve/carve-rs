//! The footnote section of a render (C2-H-089 to C2-H-093, C2-M-061 to C2-M-064).

use super::Cx;
use super::attrs::{class_list, is_raw_span};
use super::dom::{Dom, Id, js_trim};
use crate::chars::fold_label;
use std::collections::{HashMap, HashSet};

/// A note of the footnote section, by the id of its item.
#[derive(Debug, Clone)]
pub(crate) struct Note {
    pub item: Id,
    pub label: Option<String>,
    pub inline: bool,
}

pub(crate) fn is_footnote_section(dom: &Dom, id: Id) -> bool {
    dom.attr(id, "role") == Some("doc-endnotes")
        || dom
            .attr(id, "class")
            .is_some_and(|c| c.split(' ').any(|n| n == "footnotes"))
}

/// C2-M-063: the backlink of a note, as a render and as GitHub write it.
pub(crate) fn is_backlink(dom: &Dom, id: Id) -> bool {
    dom.is(id, "a")
        && (dom.attr(id, "role") == Some("doc-backlink")
            || dom.has(id, "data-footnote-backref")
            || class_list(dom, id).iter().any(|c| c == "footnote-backref"))
}

fn blank_text(dom: &Dom, id: Id) -> bool {
    dom.is_text(id) && js_trim(dom.text(id)).is_empty()
}

/// C2-H-091: the backlinks a render adds to the last paragraph of a note.
fn drop_backlinks(dom: &mut Dom, item: Id) {
    let Some(last) = dom.elements(item).last().copied() else {
        return;
    };
    if !dom.is(last, "p") {
        return;
    }
    let nodes = dom.children(last).to_vec();
    let mut end = nodes.len();
    while let Some(&n) = end.checked_sub(1).and_then(|i| nodes.get(i)) {
        if !(is_backlink(dom, n) || blank_text(dom, n)) {
            break;
        }
        end -= 1;
    }
    let tail = nodes.get(end..).unwrap_or(&[]);
    if end == nodes.len() || !tail.iter().any(|&n| is_backlink(dom, n)) {
        return;
    }
    let head = nodes.get(..end).unwrap_or(&[]);
    if end > 0 && !head.iter().all(|&n| blank_text(dom, n)) {
        dom.set_children(last, head.to_vec());
        if let Some(&t) = head.last()
            && dom.is_text(t)
        {
            let trimmed = dom.text(t).trim_end_matches([' ', '\t']).to_string();
            dom.set_text(t, trimmed);
        }
        return;
    }
    let kept: Vec<Id> = dom
        .children(item)
        .iter()
        .copied()
        .filter(|&n| n != last)
        .collect();
    dom.set_children(item, kept);
}

/// C2-H-186: why a note's `data-carve-inline-footnote` is not read.
fn inline_fault(dom: &Dom, item: Id, referenced: bool) -> Option<&'static str> {
    if dom.has(item, "data-carve-footnote-label") {
        return Some("the note also carries a footnote key");
    }
    if !referenced {
        return Some("no reference points to the note");
    }
    let blocks: Vec<Id> = dom
        .children(item)
        .iter()
        .copied()
        .filter(|&n| !dom.is_blank(n))
        .collect();
    let single = blocks.len() <= 1 && blocks.iter().all(|&n| dom.is(n, "p"));
    (!single).then_some("the note holds more than one paragraph")
}

type References = HashMap<String, HashSet<Option<String>>>;

fn note_entry(cx: &mut Cx, item: Id, references: Option<&HashSet<Option<String>>>) -> Note {
    let label = cx
        .dom
        .attr(item, "data-carve-footnote-label")
        .map(str::to_string);
    let inline = cx.dom.has(item, "data-carve-inline-footnote");
    let fault = if inline {
        inline_fault(&cx.dom, item, references.is_some())
    } else {
        None
    };
    if let Some(f) = fault {
        cx.ignored_carrier(item, "data-carve-inline-footnote", f);
    }
    if let Some(l) = &label
        && (l.is_empty() || fold_label(l) != *l || inline)
    {
        let reason = if inline {
            "the note is also an inline footnote"
        } else {
            "the key is not folded"
        };
        cx.ignored_carrier(item, "data-carve-footnote-label", reason);
        return Note {
            item,
            label: None,
            inline: false,
        };
    }
    // C2-H-186: a reference that carries another key makes both carriers disagree.
    if let Some(l) = &label
        && references
            .into_iter()
            .flatten()
            .any(|k| k.as_ref().is_some_and(|k| k != l))
    {
        cx.ignored_carrier(
            item,
            "data-carve-footnote-label",
            "a reference to the note carries another key",
        );
        return Note {
            item,
            label: None,
            inline: false,
        };
    }
    Note {
        item,
        label,
        inline: inline && fault.is_none(),
    }
}

/// Records the notes of every footnote section before the references are read.
pub(crate) fn collect(cx: &mut Cx, nodes: &[Id]) {
    let mut pending: Vec<Id> = nodes.to_vec();
    let mut sections = Vec::new();
    let mut referenced: References = HashMap::new();
    while let Some(n) = pending.pop() {
        if !cx.dom.is_element(n) || is_raw_span(&cx.dom, n) {
            continue;
        }
        if cx.dom.is(n, "section") && is_footnote_section(&cx.dom, n) {
            sections.push(n);
        } else {
            record_reference(&cx.dom, n, &mut referenced);
        }
        pending.extend(cx.dom.children(n));
    }
    for section in sections {
        collect_section(cx, section, &referenced);
    }
}

fn record_reference(dom: &Dom, id: Id, referenced: &mut References) {
    let Some(target) = dom.attr(id, "href").and_then(|h| h.strip_prefix('#')) else {
        return;
    };
    if !dom.is(id, "a") || dom.attr(id, "role") != Some("doc-noteref") {
        return;
    }
    referenced.entry(target.to_string()).or_default().insert(
        dom.attr(id, "data-carve-footnote-label")
            .map(str::to_string),
    );
}

fn collect_section(cx: &mut Cx, section: Id, referenced: &References) {
    let Some(list) = cx
        .dom
        .elements(section)
        .into_iter()
        .find(|&c| cx.dom.is(c, "ol"))
    else {
        return;
    };
    for item in cx.dom.elements(list) {
        if !cx.dom.is(item, "li") {
            continue;
        }
        let id = cx.dom.attr(item, "id").map(str::to_string);
        drop_backlinks(&mut cx.dom, item);
        if let Some(id) = id.filter(|i| !i.is_empty()) {
            let entry = note_entry(cx, item, referenced.get(&id));
            cx.notes.insert(id, entry);
        }
    }
}
