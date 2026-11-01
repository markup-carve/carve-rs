use carve::{to_html, try_to_html_with_options, Options};

#[test]
fn a_sibling_marker_after_a_lazy_fold_keeps_one_inner_list() {
    let source = "- - - a\n  p\n\n    - q\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li><p>a\np</p></li>\n          <li><p>q</p></li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim(), expected);
    assert_eq!(
        try_to_html_with_options(source, &Options::default().with_positions(true))
            .unwrap()
            .trim(),
        expected
    );
}

#[test]
fn a_note_lead_keeps_its_inner_item_after_the_note_takes_a_lazy_line() {
    let source = "r[^1]\n\n- - [^1]: x\n  p\n\n    q\n";
    let expected = "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li>\n    <ul>\n      <li>q</li>\n    </ul>\n  </li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x\np<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
    assert_eq!(
        try_to_html_with_options(source, &Options::default().with_positions(true))
            .unwrap()
            .trim(),
        expected
    );
}
