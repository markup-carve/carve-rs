//! A `#` placeholder in a PANEL caption stays the literal `#` the author wrote.
//!
//! PART 9 §4c: panels are not sequence units, so a placeholder there has
//! nothing to resolve against. It stays LITERAL - the visible failure this
//! language prefers to a silent one - and `carve lint` reports it as
//! `figure-group-panel-number`. The node stays a `caption_number` without a
//! number, the same keep-the-typed-node discipline PART 12 §3a applies to an
//! unresolved reference, so the wire and the writer keep the authored `#`.

const PANEL_PLACEHOLDER: &str = "\
::: figure
![one](a.png)
^ Panel #: One
:::
^ Figure #: Group
";

#[test]
fn the_html_target_keeps_the_literal_hash() {
    let html = carve::to_html(PANEL_PLACEHOLDER);
    assert!(
        html.contains("<figcaption>Panel #: One</figcaption>"),
        "{html}"
    );
    assert!(
        html.contains("<figcaption>Figure 1: Group</figcaption>"),
        "{html}"
    );
}

#[test]
fn the_non_html_targets_agree() {
    let markdown = carve::to_markdown(PANEL_PLACEHOLDER);
    assert!(markdown.contains("*Panel #: One*"), "{markdown}");
    let plain = carve::to_plain_text(PANEL_PLACEHOLDER);
    assert!(plain.contains("Panel #: One"), "{plain}");
}

#[test]
fn the_placeholder_never_advances_a_counter() {
    // The panel's label bucket must not exist: a later REAL `Panel #:` caption
    // outside the group starts at 1.
    let source = format!("{PANEL_PLACEHOLDER}\n```\ncode\n```\n^ Panel #: Real\n");
    let html = carve::to_html(&source);
    assert!(html.contains("Panel 1: Real"), "{html}");
}

#[test]
fn lint_reports_figure_group_panel_number() {
    let warnings = carve::lint_carve(PANEL_PLACEHOLDER);
    assert!(
        warnings
            .iter()
            .any(|w| w.rule == "figure-group-panel-number"),
        "{warnings:?}"
    );
}

#[test]
fn a_group_without_a_panel_placeholder_lints_clean() {
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
fn the_writer_writes_the_hash_back() {
    let out = carve::to_carve(PANEL_PLACEHOLDER);
    assert!(out.contains("^ Panel #: One"), "{out}");
}
