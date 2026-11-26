//! carve-rs#4849: the short forms keep the outputs all three engines agree on.

use carve::api::{Carve, WriteOptions};

fn fmt(src: &str) -> String {
    Carve::new()
        .format(src, &WriteOptions::default())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn superscript_brace_units() {
    let src = "{^\\{^^} {^\\{^^} {^\\{^^}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn inline_notes_with_an_escaped_bracket() {
    let src = "^[a{^\\[]]\n^[a{^\\[]]\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn reference_runs_with_a_backslash_label() {
    assert_eq!(fmt("[a][\\][a][\\][a][\\]\n"), "[a][\\][a]\\[]\\[a]\\[]\n");
}
