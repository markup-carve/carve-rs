fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_column_0_definition_closes_an_item_holding_a_quote() {
    assert_eq!(
        html("- > para\n[r]: /u\n[x][r]\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p><a href=\"/u\">x</a></p>"
    );
    assert_eq!(
        html("- > para\n[^p]: n\nx\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul>"
    );
    assert_eq!(
        html("- > para\n[r]: /u\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p>tail</p>"
    );
    assert_eq!(
        html("- > para\n[^p]: n\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul>"
    );
}

#[test]
fn a_column_0_note_closes_an_ordered_sub_list() {
    assert_eq!(
        html("1. 1. para\n[^p]: n\nx\n"),
        "<ol><li><ol><li>para</li></ol></li></ol>"
    );
}

#[test]
fn a_definition_at_a_description_column_closes_its_sub_list() {
    assert_eq!(
        html(":: t\n: - para\n  [r]: /u\n    tail\n"),
        "<dl><dt>t</dt><dd><ul><li>para</li></ul><p>tail</p></dd></dl>"
    );
    assert_eq!(
        html(":: t\n: - para\n   [r]: /u\n    tail\n"),
        "<dl><dt>t</dt><dd><ul><li>para</li></ul><p>tail</p></dd></dl>"
    );
}

#[test]
fn a_definition_at_an_item_column_closes_its_description() {
    assert_eq!(
        html("- :: t\n  : para\n  [r]: /u\n    tail\n"),
        "<ul><li><dl><dt>t</dt><dd>para</dd></dl> tail </li></ul>"
    );
    assert_eq!(
        html("- :: t\n  : para\n   [r]: /u\n    tail\n"),
        "<ul><li><dl><dt>t</dt><dd>para</dd></dl> tail </li></ul>"
    );
}

#[test]
fn a_definition_at_an_item_column_closes_its_sub_item() {
    assert_eq!(
        html("- - para\n  [r]: /u\n    [x][r]\n"),
        "<ul><li><ul><li>para</li></ul><a href=\"/u\">x</a></li></ul>"
    );
    assert_eq!(
        html("- - para\n  [r]: /u\n    tail\n"),
        "<ul><li><ul><li>para</li></ul> tail </li></ul>"
    );
    assert_eq!(
        html("- - para\n   [r]: /u\n    tail\n"),
        "<ul><li><ul><li>para</li></ul> tail </li></ul>"
    );
}

#[test]
fn a_column_0_definition_ends_the_quote_hosting_item() {
    assert_eq!(
        html("1. > para\n[r]: /u\n   > tail\n"),
        "<ol><li><blockquote><p>para</p></blockquote></li></ol><p>&gt; tail</p>"
    );
}

#[test]
fn a_column_0_definition_closes_an_ordered_item_colon_body() {
    assert_eq!(html("1. ::: note\n   more\n[r]: /u\n   tail\n"), "<ol><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li></ol><p>tail</p>");
}

#[test]
fn a_note_at_an_ordered_item_column_closes_its_sub_item() {
    assert_eq!(
        html("1. - para\n    [^p]: n\n     tail\n"),
        "<ol><li><ul><li>para</li></ul></li></ol>"
    );
}

#[test]
fn a_column_0_note_closes_an_ordered_item_colon_body() {
    assert_eq!(html("1. ::: note\n   para\n[^p]: n\nx\n"), "<ol><li><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p></aside></li></ol>");
}

#[test]
fn a_column_0_definition_closes_an_item_colon_body() {
    assert_eq!(html("- ::: note\n  para\n[r]: /u\ntail\n"), "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p></aside></li></ul><p>tail</p>");
}

#[test]
fn an_indented_line_after_a_column_0_definition_is_a_document_paragraph() {
    assert_eq!(
        html("1. > para\n[r]: /u\n     tail\n"),
        "<ol><li><blockquote><p>para</p></blockquote></li></ol><p>tail</p>"
    );
}

#[test]
fn a_column_0_note_closes_an_ordered_item_holding_a_quote() {
    assert_eq!(
        html("1. > para\n[^p]: n\ntail\n"),
        "<ol><li><blockquote><p>para</p></blockquote></li></ol>"
    );
}

#[test]
fn a_column_0_definition_closes_a_nested_bullet_list() {
    assert_eq!(
        html("- - para\n[r]: /u\nx\n"),
        "<ul><li><ul><li>para</li></ul></li></ul><p>x</p>"
    );
}

#[test]
fn a_column_0_definition_closes_an_ordered_sub_list() {
    assert_eq!(
        html("- 1. para\n[r]: /u\nx\n"),
        "<ul><li><ol><li>para</li></ol></li></ul><p>x</p>"
    );
}

#[test]
fn a_column_0_note_closes_a_bullet_sub_list() {
    assert_eq!(
        html("1. - para\n[^p]: n\nx\n"),
        "<ol><li><ul><li>para</li></ul></li></ol>"
    );
}

#[test]
fn a_column_0_note_closes_an_ordered_item_colon_body_before_tail() {
    assert_eq!(html("1. ::: note\n   more\n[^p]: n\ntail\n"), "<ol><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li></ol>");
}

#[test]
fn a_note_at_a_description_column_closes_its_ordered_sub_list() {
    assert_eq!(
        html(":: t\n: 1. para\n    [^p]: n\n     tail\n"),
        "<dl><dt>t</dt><dd><ol><li>para</li></ol></dd></dl>"
    );
}

#[test]
fn a_definition_at_an_ordered_item_column_closes_its_sub_item() {
    assert_eq!(
        html("1. 1. para\n     [r]: /u\n      tail\n"),
        "<ol><li><ol><li>para</li></ol> tail </li></ol>"
    );
}

#[test]
fn a_column_0_definition_closes_an_item_colon_body_before_an_indented_line() {
    assert_eq!(html("- ::: note\n  more\n[r]: /u\n  tail\n"), "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li></ul><p>tail</p>");
}
