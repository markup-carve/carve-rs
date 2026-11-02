#[test]
fn definitions_and_comments_do_not_split_list_siblings() {
    let source = "- a\n[r]: /u\n%% c\n- b\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn invisible_lines_do_not_split_description_markers() {
    let source = ":: t\n%% c\n[r]: /u\n:: u\n: d\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dt>u</dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn three_blank_lines_still_separate_sibling_lists() {
    let source = "- a\n\n\n\n[r]: /u\n- b\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn abbreviation_between_siblings_keeps_one_list_and_its_definition() {
    let source = "- A\n*[A]: alpha\n- A\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><abbr title=\"alpha\">A</abbr></li>\n  <li><abbr title=\"alpha\">A</abbr></li>\n</ul>"
    );
    let document = carve::parse(source);
    assert_eq!(document.children.len(), 2);
    assert!(matches!(document.children[0], carve::BlockNode::List(_)));
    assert!(matches!(document.children[1], carve::BlockNode::AbbreviationDef(_)));
}

#[test]
fn abbreviation_between_definition_markers_keeps_the_description() {
    let source = ":: A\n*[A]: alpha\n: d\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt><abbr title=\"alpha\">A</abbr></dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn invisible_line_does_not_reset_three_blank_definition_boundary() {
    let source = ":: a\n\n\n%% c\n\n:: b\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>a</dt>\n</dl>\n<dl>\n  <dt>b</dt>\n</dl>"
    );
}

#[test]
fn later_abbreviation_definition_still_wins_after_a_skipped_one() {
    let source = "- A\n*[A]: first\n- A\n*[A]: second\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><abbr title=\"second\">A</abbr></li>\n  <li><abbr title=\"second\">A</abbr></li>\n</ul>"
    );
}

#[test]
fn an_invisible_line_in_a_nested_note_does_not_bridge_description_siblings() {
    let source = ":: t\n[^f]: a\n     [r]: /url\n:  - a\n    [^n]: note text\n\nSee [^n].\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n</dl>\n<p>:  - a</p>\n<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>.</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
