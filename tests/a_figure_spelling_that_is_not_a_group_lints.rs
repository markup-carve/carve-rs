//! The `::: figure` spellings that are NOT a composite figure are diagnosed.
//!
//! PART 9 §4c, LINT NOT PARSE: an opener carrying a quoted title or a
//! `[label]` stays a generic container (`figure-group-opener-metadata`), and a
//! bare opener nested inside an open group's body - at any depth - stays one
//! too (`figure-group-nested`). Both are warnings over a valid parse; the
//! parse itself is pinned by corpus 318-composite-figures-8 and -9.

#[test]
fn an_opener_with_a_title_reports_opener_metadata() {
    let warnings = carve::lint_carve("::: figure \"A titled figure div\"\nBody.\n:::\n");
    assert!(
        warnings
            .iter()
            .any(|w| w.rule == "figure-group-opener-metadata"),
        "{warnings:?}"
    );
}

#[test]
fn an_opener_with_a_label_reports_opener_metadata() {
    let warnings = carve::lint_carve("::: figure [g]\nBody.\n:::\n");
    assert!(
        warnings
            .iter()
            .any(|w| w.rule == "figure-group-opener-metadata"),
        "{warnings:?}"
    );
}

#[test]
fn a_nested_bare_opener_reports_nested() {
    let source = "\
::: figure
:::: figure
![one](a.png)
^ (a) One
::::
:::
^ Figure #: Outer only
";
    let warnings = carve::lint_carve(source);
    assert!(
        warnings.iter().any(|w| w.rule == "figure-group-nested"),
        "{warnings:?}"
    );
}

#[test]
fn the_nesting_rule_reaches_any_depth() {
    // Through an intermediate generic div: still inside the open group's body.
    let source = "\
::: figure
:::: note
::::: figure
text
:::::
::::
:::
";
    let warnings = carve::lint_carve(source);
    assert!(
        warnings.iter().any(|w| w.rule == "figure-group-nested"),
        "{warnings:?}"
    );
}

#[test]
fn a_bare_group_outside_a_group_lints_clean() {
    let source = "\
::: figure
![one](a.png)
^ (a) One
:::
^ Figure #: Group
";
    assert_eq!(carve::lint_carve(source), vec![]);
}

#[test]
fn a_generic_kind_reports_nothing() {
    // `::: sidebar "T"` is an ordinary titled container; only the reserved
    // kind word gets the diagnostic.
    let warnings = carve::lint_carve("::: sidebar \"T\"\nBody.\n:::\n");
    assert_eq!(warnings, vec![]);
}
