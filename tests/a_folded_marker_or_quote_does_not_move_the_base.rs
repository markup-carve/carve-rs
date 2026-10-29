//! A list-shaped line folded into a body's paragraph opens no item (PART 9 §10
//! I2), and a block quote does not rebase (CARVE-P0-004). A block at a quoted
//! description body's column leaves no paragraph open (CARVE-P0-011, P0-012).

fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_heading_past_the_description_column_opens_after_a_folded_marker() {
    assert_eq!(
        html(":: t\n: d\n  - para\n    # H\nx\n"),
        "<dl><dt>t</dt><dd><p>d - para</p><h1 id=\"H\">H</h1></dd></dl><p>x</p>"
    );
}

#[test]
fn an_attribute_line_past_the_description_column_ends_the_paragraph() {
    assert_eq!(
        html(":: t\n: d\n  - para\n    {#k}\nx\n"),
        "<dl><dt>t</dt><dd>d - para</dd></dl><p>x</p>"
    );
}

#[test]
fn a_heading_past_the_footnote_column_opens_after_a_folded_marker() {
    let got = html("[^n]: d\n  - para\n    # H\nx\n\n[^n]\n");
    assert!(got.starts_with("<p>x</p>"), "{got}");
    assert!(
        got.contains("<li id=\"fn1\"><p>d - para</p><h1 id=\"H\">H</h1><p><a href=\"#fnref1\""),
        "{got}"
    );
}

#[test]
fn the_definition_spelling_still_registers() {
    assert_eq!(
        html(":: t\n: d\n  - para\n    [r]: /u\nx\n\n[r][]\n"),
        "<dl><dt>t</dt><dd>d - para</dd></dl><p>x</p><p><a href=\"/u\">r</a></p>"
    );
}

#[test]
fn an_attribute_line_past_the_quote_marker_is_quoted_text() {
    assert_eq!(
        html("- > para\n  >  {#x}\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p>tail</p>"
    );
}

#[test]
fn an_attribute_line_past_a_nested_quote_marker_is_quoted_text() {
    assert_eq!(
        html("> - > para\n>   > more\n>   >  {#k}\ntail\n"),
        "<blockquote><ul><li><blockquote><p>para more {<span class=\"tag\"><strong>#k</strong></span>} tail</p></blockquote></li></ul></blockquote>"
    );
}

#[test]
fn an_attribute_line_at_the_quote_column_still_ends_the_paragraph() {
    assert_eq!(
        html("- > para\n  > {#x}\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p>tail</p>"
    );
}

#[test]
fn a_block_in_a_quoted_description_body_lets_tail_leave_the_quote() {
    assert_eq!(
        html("> :: t\n> : d\n>   {#x}\ntail\n"),
        "<blockquote><dl><dt>t</dt><dd>d</dd></dl></blockquote><p>tail</p>"
    );
    assert_eq!(
        html("> :: t\n> : d\n>   # H\ntail\n"),
        "<blockquote><dl><dt>t</dt><dd><p>d</p><h1 id=\"H\">H</h1></dd></dl></blockquote><p>tail</p>"
    );
}
