fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_quote_past_the_column_opens_after_a_blank_separated_paragraph() {
    assert_eq!(
        html("- a\n\n  before\n   > nested\n"),
        "<ul><li><p>a</p><p>before</p><blockquote><p>nested</p></blockquote></li></ul>"
    );
}

#[test]
fn a_quote_past_the_column_opens_after_a_comment() {
    assert_eq!(
        html("- a\n  %% c\n  before\n   > nested\n"),
        "<ul><li>a before <blockquote><p>nested</p></blockquote></li></ul>"
    );
}

#[test]
fn a_quote_past_the_column_opens_after_an_attribute_or_heading() {
    assert_eq!(
        html("- :: term\n  {.x}\n  before\n   > nested\n"),
        "<ul><li><dl><dt>term</dt></dl><p class=\"x\">before</p><blockquote><p>nested</p></blockquote></li></ul>"
    );
    assert_eq!(
        html("- :: term\n  # H\n  before\n   > nested\n"),
        "<ul><li><dl><dt>term</dt></dl><h1 id=\"H\">H</h1> before <blockquote><p>nested</p></blockquote></li></ul>"
    );
}

#[test]
fn an_indented_plus_stays_in_the_nested_description() {
    let source = "- :: term\n  : body\n  +\n  before\n   > nested\n";
    super::a_continuation_marker_is_column_zero_only::assert_legacy_model(
        source,
        &carve::to_html(source),
    );
}
