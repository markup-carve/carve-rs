//! An invisible line or comment span is read as if it were absent
//! (CARVE-P9-013 property 3): it keeps a description entry open, and a blank in
//! front of it still separates (markup-carve/carve#4901, #4903).

fn render(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_comment_after_a_description_keeps_the_quote_continuation() {
    assert_eq!(
        render(":: t\n:  d\n%%\n:  > a\n   > b\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n  <dd>\n    <blockquote><p>a\nb</p></blockquote>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_comment_span_after_a_description_keeps_a_shifted_sibling() {
    assert_eq!(
        render(":: t\n: d\n%%%\nc\n%%%\n : d2\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n  <dd>d2</dd>\n</dl>"
    );
}

#[test]
fn a_comment_span_after_a_term_in_an_item_keeps_the_description() {
    assert_eq!(
        render("- :: t\n  :: t2\n  %%%\n  c\n  %%%\n  :  > a\n     > b\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dt>t2</dt>\n      <dd>\n        <blockquote><p>a\nb</p></blockquote>\n      </dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn a_blank_before_a_comment_in_a_description_body_loosens_the_list() {
    assert_eq!(
        render("- :: term\n  : payload\n\n    %% c\n- sub\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>term</dt>\n      <dd>payload</dd>\n    </dl>\n  </li>\n  <li><p>sub</p></li>\n</ul>"
    );
}

#[test]
fn a_blank_before_a_definition_in_a_nested_list_loosens_the_list() {
    assert_eq!(
        render("- a\n  - b\n\n    [r]: /u\n- sub\n"),
        "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n  <li><p>sub</p></li>\n</ul>"
    );
}

#[test]
fn a_visible_line_after_the_comment_keeps_the_blank_inside_the_item() {
    let html = render("- :: term\n  : payload\n\n    %% c\n    more\n- sub\n");
    assert!(html.contains("<li>sub</li>"), "{html}");
}

#[test]
fn a_comment_span_after_a_sub_list_keeps_the_list_whole() {
    assert_eq!(
        render("- a\n  - b\n  %%%\n  c\n  %%%\n- sub\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n  <li>sub</li>\n</ul>"
    );
}

#[test]
fn a_blank_then_a_comment_span_then_a_paragraph_loosens_the_item() {
    assert_eq!(
        render("- a\n  - b\n\n  %%%\n  c\n  %%%\n  more\n- sub\n"),
        "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>b</li>\n    </ul>\n    <p>more</p>\n  </li>\n  <li><p>sub</p></li>\n</ul>"
    );
}

#[test]
fn the_writer_keeps_a_tight_item_tight_across_a_comment_after_a_sub_list() {
    let source = "- - x\n  %%%\n  y\n";
    let formatted = carve::to_carve(source);
    assert_eq!(
        carve::to_html(&formatted),
        carve::to_html(source),
        "{formatted}"
    );
}
