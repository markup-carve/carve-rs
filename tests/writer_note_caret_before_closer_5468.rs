//! carve-js#5468, C2-W-125, C2-W-091: a note whose text ends in `]` behind a caret glued to a link
//! keeps the escape of its caret, which only the escaped closer behind it needs, and is written
//! as itself with no loss.

use carve::api::{Carve, Options, WriteOptions};

fn fmt(src: &str) -> (String, Vec<String>) {
    let carve = Carve::new();
    let tree = carve.parse(src, &Options::default()).expect("a parse");
    let written = carve
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("a write");
    let codes = written
        .diagnostics
        .iter()
        .map(|e| e.code.to_string())
        .collect();
    (written.output, codes)
}

#[test]
fn a_note_behind_a_caret_glued_to_a_link_is_written_as_itself() {
    for src in [
        "^[a\\^[b][[b]\\]]\n",
        "^[\\^[b][[b]\\]]\n",
        "^[a\\^[b][[b]\\][b]]\n",
        "x ^[ \\^[b][[^b]\\]] [x]\n",
    ] {
        assert_eq!(fmt(src), (src.to_string(), Vec::new()), "{src:?}");
    }
}

#[test]
fn a_caret_before_the_note_is_escaped_and_the_tree_reads_back() {
    let (out, losses) = fmt("^[^[a\\^[b][[b]\\]]\n");
    assert!(losses.is_empty(), "{losses:?}");
    assert_eq!(out, "^\\[^[a\\^[b][[b]\\]]\n");
}
