//! Djot import of carve#6695: `]` before a `{` that opens a delimiter closes the span (djot.js); a plain
//! `{` makes the span only when its attribute block is valid (jgm/djot.js 00eb258, as carve-js).

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, WriteOptions};

fn import(source: &str) -> String {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            source,
            ImportFormat::Djot,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds");
    assert!(
        imported
            .diagnostics
            .iter()
            .all(|e| e.channel != "render-loss"),
        "{source:?}: {:?}",
        imported.diagnostics
    );
    carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output
}

#[test]
fn a_brace_that_opens_a_delimiter_still_closes_the_span() {
    let cases = [
        ("[r]{=x}\n", "[r]{}{=x}\n"),
        ("[a]{_b_}\n", "[a]{}/b/\n"),
        ("![a]{=x}\n", "![a]{}{=x}\n"),
        ("[a]{'b'}\n", "[a]{}\u{2018}b\u{2019}\n"),
        ("[a]{-b-}\n", "[a]{}{-b-}\n"),
        ("[a]{}\n", "[a]{}\n"),
        ("![a]{}\n", "![a]{}\n"),
        ("[a]{.c}\n", "[a]{.c}\n"),
        ("[a]{.c\n", "[a]{.c\n"),
        ("[]{\n", "[]{\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(import(source), expected, "{source:?}");
    }
}
