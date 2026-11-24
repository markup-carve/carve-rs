use carve::api::{Carve, Options, WriteOptions};

fn assert_writes(cases: &[(&str, &str)]) {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for &(source, expected) in cases {
        let tree = carve.parse(source, &Options::default()).unwrap();
        assert_eq!(
            carve.write(&tree, &options).unwrap(),
            expected,
            "{source:?}"
        );
        assert_eq!(
            carve.format(expected, &options).unwrap(),
            expected,
            "{expected:?}"
        );
    }
}

/// carve-rs#4766: the link fallback compares the round again with the respell steps unspent.
#[test]
fn brace_before_strong_in_link_text_takes_the_backslash() {
    assert_writes(&[
        ("[x\\{*a*]()\n", "[x\\{*a*]()\n"),
        ("[{{*a*}]()\n", "[\\{*a*]()\n"),
    ]);
}

/// carve-rs#4763: C2-B-224 does not list a footnote label, so its key keeps `\|`.
#[test]
fn footnote_label_in_cell_keeps_the_escaped_pipe() {
    assert_writes(&[
        ("|[^\\|]|\n", "| [^\\|] |\n"),
        ("|[^a\\|b]| x |\n", "| [^a\\|b] | x |\n"),
    ]);
}

/// carve#6575: a code span that opens in a note or span and closes behind it is repaired by kind (C2-W-088).
#[test]
fn backtick_behind_note_or_span_takes_the_backslash() {
    assert_writes(&[
        ("^[\\` ];`\n", "^[\\` ];\\`\n"),
        ("^[\\`^]`\n", "^[\\`^]\\`\n"),
        ("``a[\\`a]()a`\n", "\\`\\`a[\\`a]()a\\`\n"),
        ("[\\` ]{.c};`\n", "[\\` ]{.c};\\`\n"),
    ]);
}
