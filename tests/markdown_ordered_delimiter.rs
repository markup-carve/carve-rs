//! In CommonMark a change of ordered-list delimiter SEPARATES two adjacent lists,
//! exactly as a change of bullet does. Measured against commonmark.js: `1. a`
//! followed by `1) c` gives two `<ol>` elements; the same input with one delimiter
//! gives one.
//!
//! So normalizing `1)` to `1.` merges lists the source kept apart -- the same
//! defect the bullet marker had (carve-rs#307), left in place a few lines below
//! the comment explaining why bullets must not be normalized (carve#352, corpus
//! 31).

#[test]
fn a_paren_delimiter_survives() {
    assert_eq!(carve::to_markdown("1) one\n2) two\n"), "1) one\n2) two\n");
}

#[test]
fn a_dot_delimiter_survives() {
    assert_eq!(carve::to_markdown("1. one\n2. two\n"), "1. one\n2. two\n");
}

#[test]
fn two_adjacent_lists_stay_apart() {
    let out = carve::to_markdown("1. a\n2. b\n\n1) c\n2) d\n");
    assert!(out.contains("1. a"), "got: {out:?}");
    assert!(out.contains("1) c"), "got: {out:?}");
}

#[test]
fn an_explicit_start_works_with_either_delimiter() {
    assert_eq!(
        carve::to_markdown("3) three\n4) four\n"),
        "3) three\n4) four\n"
    );
    assert_eq!(
        carve::to_markdown("3. three\n4. four\n"),
        "3. three\n4. four\n"
    );
}

#[test]
fn markdown_import_keeps_the_authored_ordered_delimiter() {
    let cases = [
        ("10) foo\n    - bar\n", "10) foo\n    - bar\n"),
        ("10) foo\n   - bar\n", "10) foo\n\n- bar\n"),
        ("1. foo\n2. bar\n3) baz\n", "1. foo\n2. bar\n\n3) baz\n"),
        ("1) one\n2) two\n", "1) one\n2) two\n"),
        ("1. one\n2. two\n", "1. one\n2. two\n"),
        ("- outer\n  1) inner\n", "- outer\n  1) inner\n"),
        ("> 1) quoted\n", "> 1) quoted\n"),
        ("1)\n", "1)\n"),
    ];
    for (source, expected) in cases {
        let imported = carve::migrate_markdown(source);
        assert_eq!(imported.value, expected, "{source:?}");
        assert_eq!(carve::try_markdown_to_carve(source).unwrap(), expected);
        assert_eq!(carve::to_carve(expected), expected);
        assert!(
            imported
                .report
                .diagnostics
                .iter()
                .all(|diagnostic| { diagnostic.fidelity == carve::MigrationFidelity::Preserved }),
            "{source:?}: {:?}",
            imported.report.diagnostics
        );
        assert_eq!(
            carve::render_html(&carve::markdown_to_ast(source)).unwrap(),
            carve::to_html(expected),
            "{source:?}"
        );
    }
}

#[test]
fn markdown_ast_retains_the_paren_delimiter() {
    let document = carve::markdown_to_ast("10) foo\n");
    let carve::BlockNode::List(list) = &document.children[0] else {
        panic!("expected an ordered list");
    };
    assert_eq!(list.delim, Some(')'));
    assert_eq!(
        carve::render_html(&document).unwrap(),
        "<ol start=\"10\" data-delim=\")\">\n  <li>foo</li>\n</ol>"
    );
}
