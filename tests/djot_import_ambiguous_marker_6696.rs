//! Djot import of carve#6696: a single-letter `ivxlcdm` marker stays roman or alphabetic until a
//! later item narrows it, and items are never compared by number, as in djot.js.

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
    carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output
}

#[test]
fn an_ambiguous_marker_takes_the_style_of_the_list() {
    let cases = [
        ("a. one\n\ni. two\n", "a. one\n\nb. two\n"),
        ("A. one\nI. two\n", "A.  one\nB.  two\n"),
        ("i. one\n\na. two\n", "{start=9}\na. one\n\nb. two\n"),
        (
            "i. one\n\n   more\n\nj. two\n",
            "{start=9}\na. one\n\n   more\n\nb. two\n",
        ),
        ("a. one\nx. two\n", "a. one\nb. two\n"),
        ("i. one\nv. two\n", "i. one\nii. two\n"),
        ("i. one\nii. two\n", "i. one\nii. two\n"),
        ("I. one\nII. two\n", "I.  one\nII.  two\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(import(source), expected, "{source:?}");
    }
}

#[test]
fn a_different_delimiter_still_starts_a_new_list() {
    assert_eq!(import("a. one\ni) two\n"), "a. one\n\n\n\ni. two\n");
}
