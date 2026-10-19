//! `consumed-key-near-miss`: a bare key one edit from the consumed key its block reads.

use super::{walk_all_blocks, warning, LintWarning};
use crate::ast::{AttrSlot, Attrs, BlockNode, Document, FigureTarget, Pos};

pub(super) fn collect(
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let mut visit = |block: &BlockNode| {
        let (attrs, pos, key) = match block {
            BlockNode::List(list) => (&list.attrs, &list.pos, "loose"),
            BlockNode::DefinitionList(list) => (&list.attrs, &list.pos, "loose"),
            BlockNode::CodeBlock(code) if is_delimited(code.lang.as_deref()) => {
                (&code.attrs, &code.pos, "table")
            }
            BlockNode::Figure(figure) => match &*figure.target {
                FigureTarget::CodeBlock(code) if is_delimited(code.lang.as_deref()) => {
                    (&figure.attrs, &figure.pos, "table")
                }
                _ => return,
            },
            _ => return,
        };
        if let Some(attrs) = attrs {
            report(attrs, pos, key, to_byte, out);
        }
    };
    walk_all_blocks(&doc.children, &mut visit);
    for body in doc.footnote_defs.values() {
        walk_all_blocks(body, &mut visit);
    }
}

fn is_delimited(lang: Option<&str>) -> bool {
    matches!(lang, Some("csv" | "tsv"))
}

fn report(
    attrs: &Attrs,
    pos: &Option<Pos>,
    key: &str,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    for slot in &attrs.order {
        let AttrSlot::Key(word) = slot else { continue };
        if attrs.key_values.get(word).is_some_and(String::is_empty) && is_near_miss(word, key) {
            out.push(warning(
                pos.clone(),
                to_byte,
                "consumed-key-near-miss",
                format!("\"{word}\" is not a consumed key here; did you mean {{{key}}}?"),
            ));
        }
    }
}

/// A case-only match, or one insertion, deletion, substitution or adjacent swap.
fn is_near_miss(word: &str, key: &str) -> bool {
    if word == key {
        return false;
    }
    if word.to_ascii_lowercase() == key {
        return true;
    }
    let (w, k) = (word.as_bytes(), key.as_bytes());
    if w.len().abs_diff(k.len()) > 1 {
        return false;
    }
    let mut d: Vec<Vec<usize>> = (0..=w.len())
        .map(|i| {
            (0..=k.len())
                .map(|j| {
                    if i == 0 {
                        j
                    } else if j == 0 {
                        i
                    } else {
                        0
                    }
                })
                .collect()
        })
        .collect();
    for i in 1..=w.len() {
        for j in 1..=k.len() {
            let cost = usize::from(w[i - 1] != k[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && w[i - 1] == k[j - 2] && w[i - 2] == k[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[w.len()][k.len()] <= 1
}
