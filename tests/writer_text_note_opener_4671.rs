//! carve-rs#4671: a text `^[` whose span keeps a caret pair of the tree ordinary (C2-I-089). The
//! writer escapes its bracket where a node behind it does not form otherwise (C2-W-090, C2-W-125),
//! and keeps a `\[^]` the first round placed (CC-1716).

use carve::api::{Carve, Options, WriteOptions};

fn write(tree: &carve::api::Tree) -> (String, Vec<String>) {
    let written = Carve::new()
        .write_with_diagnostics(tree, &WriteOptions::default())
        .expect("a write");
    let codes = written
        .diagnostics
        .iter()
        .map(|e| e.code.to_string())
        .collect();
    (written.output, codes)
}

fn fmt(src: &str) -> (String, Vec<String>) {
    let tree = Carve::new()
        .parse(src, &Options::default())
        .expect("a parse");
    write(&tree)
}

#[test]
fn a_text_note_opener_before_a_footnote_reference_takes_the_backslash() {
    for (src, out) in [
        ("^[[^[\\^[b][[b]\\]]\n", "^\\[[^[\\^[b]\\[[b]]]\n"),
        ("^[[^[a\\^[b][[b]\\]]\n", "^\\[[^[a\\^[b]\\[[b]]]\n"),
        ("^[[^[a\\[^[b][[b]\\]]\n", "^\\[[^[a\\[^[b]\\[[b]]]\n"),
    ] {
        assert_eq!(fmt(src), (out.to_string(), Vec::new()), "{src:?}");
        assert_eq!(fmt(out), (out.to_string(), Vec::new()), "{out:?}");
    }
}

#[test]
fn an_empty_reference_the_first_round_escaped_keeps_its_backslash() {
    // CC-1716: bare, `[^]` opens a node, so its backslash is the procedure's.
    assert_eq!(
        fmt("^[^[[^]\\[]\n"),
        ("^\\[^\\[\\[^]\\[]\n".to_string(), Vec::new())
    );
    // Read bare, the `^\[` here closes as a note, so the `[^]` inside opens nothing.
    assert_eq!(
        fmt("^\\[x \\[^]]\n"),
        ("^\\[x [^]]\n".to_string(), Vec::new())
    );
}
