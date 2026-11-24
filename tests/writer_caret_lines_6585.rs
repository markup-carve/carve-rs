//! markup-carve/carve#6585 A1: canonical output passes `unattached-caption` (C2-W-146, C2-W-092).

use carve::api::{Carve, WriteOptions};
use carve::lint::{LintOptions, lint};

#[test]
fn a_caret_line_that_stays_text_takes_a_backslash_in_any_container() {
    let cases = [
        (
            "| a | b |\n^ first\n\n^ second\n",
            "| a | b |\n^ first\n\n\\^ second\n",
        ),
        ("> a\n> ^ cap\n", "> a\n> \\^ cap\n"),
        ("- a\n  ^ x\n", "- a\n  \\^ x\n"),
        ("x[^1]\n\n[^1]: a\n  ^ x\n", "x[^1]\n\n[^1]: a\n  \\^ x\n"),
        ("term\n: def\n^ x\n", "term\n: def\n\\^ x\n"),
        ("a\n^ *s*\n", "a\n\\^ *s*\n"),
        ("a *b\n^ c*\n", "a *b\n\\^ c*\n"),
        // Near cases: a caption and a caret without a space keep their form.
        ("| a |\n^ cap\n", "| a |\n^ cap\n"),
        ("a\n^x\n", "a\n^x\n"),
    ];
    let carve = Carve::new();
    for (input, expected) in cases {
        let written = carve.format(input, &WriteOptions::default()).unwrap();
        assert_eq!(written, expected, "{input:?}");
        let found = lint(&written, &LintOptions::default());
        assert!(
            found.iter().all(|d| d.code != "unattached-caption"),
            "{input:?}: {found:?}"
        );
    }
}
