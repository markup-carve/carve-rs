//! Djot import of carve#6665: a `]` with no `(`, `[` or `{` behind it closes no bracket, as in djot.js.

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
fn a_lone_closing_bracket_stays_text() {
    let cases = [
        ("[a [b] c](x)\n", "\\[a [b\\] c](x)\n"),
        ("![a [b] c](x)\n", "!\\[a [b\\] c](x)\n"),
        ("[a [b] c][r]\n\n[r]: /u\n", "\\[a [b\\] c](/u)\n"),
        ("x [a] [b](y) z\n", "x [a] [b](y) z\n"),
        ("[a]{.c} b\n", "[a]{.c} b\n"),
        ("[a [b]{.k} c](x)\n", "[a [b]{.k} c](x)\n"),
        ("_a [b] c_\n", "/a [b] c/\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(import(source), expected, "{source:?}");
    }
}
