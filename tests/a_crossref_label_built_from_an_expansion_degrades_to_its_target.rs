//! A cross-reference whose target heading's label is nothing but an abbreviation
//! span degrades to the AUTHORED TARGET once the shared expansion budget is
//! spent, not to the empty string the degraded expansion leaves behind
//! (markup-carve/carve-rs#2130).
//!
//! The label and the expansion charge ONE budget, so past exhaustion the label
//! renders empty and an empty label costs nothing. Measuring that render accepted
//! it. carve-js `8ad6d7691` and carve-php `ab0648469` both write the target on
//! every expansion size measured, so the empty label was this engine alone.

const REFERENCES: usize = 20;

/// The heading's whole label is one abbreviation span, which is what puts the
/// label's own bytes on the budget the label charge is tested against.
fn source(expansion: usize) -> String {
    format!(
        "{{#e}}\n# []{{abbr={}}}\n\n{}\n",
        "x".repeat(expansion),
        vec!["</#e>"; REFERENCES].join(" ")
    )
}

/// The last 40 bytes, so a failure names what was written without dumping the
/// whole expansion into the log.
fn tail(rendered: &str) -> &str {
    &rendered[rendered.len().saturating_sub(40)..]
}

#[test]
fn a_spent_budget_writes_the_target_rather_than_an_empty_label() {
    let doc = carve::parse(&source(30_000));

    let markdown = carve::render_markdown(&doc).unwrap();
    assert!(
        !markdown.contains("[](#)"),
        "a degraded Markdown reference wrote an empty label: {:?}",
        tail(&markdown)
    );
    assert!(
        markdown.ends_with("[e](#) [e](#) [e](#) [e](#)\n"),
        "{:?}",
        tail(&markdown)
    );

    let plain = carve::render_plain_text(&doc).unwrap();
    assert!(plain.ends_with(") e e e e\n"), "{:?}", tail(&plain));

    // The terminal target wraps each label in its own colour run, so an empty
    // label is an opener butted against its reset.
    let ansi = carve::render_ansi(&doc).unwrap();
    assert!(
        !ansi.contains("[34m\u{1b}[0m"),
        "a degraded ANSI reference wrote an empty label: {:?}",
        tail(&ansi)
    );
    assert!(ansi.ends_with("e\u{1b}[0m\n"), "{:?}", tail(&ansi));
}

/// The HTML target never had the defect: its abbreviation arm writes the `title`
/// attribute whatever the budget says, so that label never empties. Pinned so a
/// later change to the charge cannot quietly move it either.
#[test]
fn the_html_target_keeps_the_label_it_always_wrote() {
    let html = carve::to_html(&source(30_000));
    assert_eq!(
        html.matches("<abbr title=").count(),
        REFERENCES + 1,
        "the heading and every reference carry the expansion"
    );
}
