//! List markers below an open term or a paragraph in a div (C2-B-015, C2-B-024,
//! C2-B-069, C2-B-075): JS and PHP read them as text of the open leaf.

use carve::lint::{LintOptions, lint};

#[test]
fn list_marker_inside_the_item_stays_in_the_term() {
    let html = carve::render(
        "a. :: key [link](u)\n:term\n  a. $$x$$\n",
        &carve::Options::default(),
    );
    assert_eq!(
        html,
        "<ol type=\"a\">\n  <li>\n    <dl>\n      <dt>key <a href=\"u\">link</a>\n:term\na. $$x$$</dt>\n    </dl>\n  </li>\n</ol>"
    );
}

#[test]
fn sibling_marker_still_ends_the_term() {
    let html = carve::render("a. :: key\nb. next\n", &carve::Options::default());
    assert_eq!(
        html,
        "<ol type=\"a\">\n  <li>\n    <dl>\n      <dt>key</dt>\n    </dl>\n  </li>\n  <li>next</li>\n</ol>"
    );
}

#[test]
fn nested_marker_in_a_div_in_an_item_is_a_lazy_line() {
    let source = "[^n]: * :::\n         [^n]: [^n]: word [ ]\n       1. text # x\n          + x | y |\n\n              ^ x\n+ x |\n \n:: more text [link](u)\n\n## H #\n";
    let found: Vec<_> = lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "list-marker-without-blank-line")
        .map(|finding| {
            let pos = finding.pos.unwrap();
            (pos.start_offset, pos.end_offset)
        })
        .collect();
    assert_eq!(found, [(49, 60)]);
}
