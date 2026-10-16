fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn heading_at_description_column_opens_beside_nested_ordered_item() {
    assert_eq!(
        html(":: t\n: 1. para\n  # H\ntail\n"),
        "<dl><dt>t</dt><dd><ol><li>para</li></ol><h1 id=\"H\">H</h1></dd></dl><p>tail</p>"
    );
}

#[test]
fn bullet_at_item_column_opens_beside_nested_description() {
    assert_eq!(
        html("- :: t\n  : para\n  - li\ntail\n"),
        "<ul><li><dl><dt>t</dt><dd>para</dd></dl><ul><li>li tail</li></ul></li></ul>"
    );
}

#[test]
fn thematic_break_below_description_column_folds_into_nested_item() {
    assert_eq!(
        html(":: t\n: - para\n ---\n"),
        "<dl><dt>t</dt><dd><ul><li>para —</li></ul></dd></dl>"
    );
}

#[test]
fn heading_below_a_description_in_an_ordered_item_folds() {
    assert_eq!(
        html("1. :: t\n   : para\n # H\n"),
        "<ol><li><dl><dt>t</dt><dd>para # H</dd></dl></li></ol>"
    );
}

#[test]
fn thematic_break_below_an_ordered_item_in_a_description_folds() {
    assert_eq!(
        html(":: t\n: 1. para\n   ---\n"),
        "<dl><dt>t</dt><dd><ol><li>para —</li></ol></dd></dl>"
    );
}

#[test]
fn attribute_at_a_bullet_column_ends_the_description_fold() {
    assert_eq!(
        html(":: t\n: - para\n    {#x}\nx\n"),
        "<dl><dt>t</dt><dd><ul><li>para</li></ul></dd></dl><p>x</p>"
    );
}

#[test]
fn thematic_break_below_an_ordered_item_keeps_its_following_text() {
    assert_eq!(
        html(":: t\n: 1. para\n    ---\nx\n"),
        "<dl><dt>t</dt><dd><ol><li>para — x</li></ol></dd></dl>"
    );
}

#[test]
fn attribute_below_an_ordered_item_column_is_text() {
    assert_eq!(html(":: t\n: 1. para\n   {#x}\n"), "<dl><dt>t</dt><dd><ol><li>para {<span class=\"tag\"><strong>#x</strong></span>}</li></ol></dd></dl>");
}

#[test]
fn attribute_at_a_description_column_ends_the_ordered_item_fold() {
    assert_eq!(
        html("1. :: t\n   : para\n     {#x}\ntail\n"),
        "<ol><li><dl><dt>t</dt><dd>para</dd></dl></li></ol><p>tail</p>"
    );
}

#[test]
fn thematic_break_below_a_description_in_a_bullet_item_folds() {
    assert_eq!(
        html("- :: t\n  : para\n ---\n"),
        "<ul><li><dl><dt>t</dt><dd>para —</dd></dl></li></ul>"
    );
}

#[test]
fn table_below_a_bullet_item_in_a_description_folds() {
    assert_eq!(
        html(":: t\n: - para\n   | a |\n"),
        "<dl><dt>t</dt><dd><ul><li>para | a |</li></ul></dd></dl>"
    );
}

#[test]
fn attribute_below_a_bullet_item_column_is_text() {
    assert_eq!(html(":: t\n: - para\n   {#x}\n"), "<dl><dt>t</dt><dd><ul><li>para {<span class=\"tag\"><strong>#x</strong></span>}</li></ul></dd></dl>");
}

#[test]
fn bullet_below_a_description_in_a_bullet_item_folds() {
    assert_eq!(
        html("- :: t\n  : para\n   - li\n"),
        "<ul><li><dl><dt>t</dt><dd>para - li</dd></dl></li></ul>"
    );
}

#[test]
fn bullet_below_a_description_in_an_ordered_item_folds() {
    assert_eq!(
        html("1. :: t\n   : para\n    - li\n"),
        "<ol><li><dl><dt>t</dt><dd>para - li</dd></dl></li></ol>"
    );
}

#[test]
fn attribute_below_an_inner_description_column_is_text() {
    assert_eq!(html(":: t\n: :: t\n  : para\n {#x}\n"), "<dl><dt>t</dt><dd><dl><dt>t</dt><dd>para {<span class=\"tag\"><strong>#x</strong></span>}</dd></dl></dd></dl>");
}

#[test]
fn attribute_below_a_description_in_a_bullet_item_is_text() {
    assert_eq!(html("- :: t\n  : para\n {#x}\n"), "<ul><li><dl><dt>t</dt><dd>para {<span class=\"tag\"><strong>#x</strong></span>}</dd></dl></li></ul>");
}

#[test]
fn attribute_below_a_description_in_an_ordered_item_is_text() {
    assert_eq!(html("1. :: t\n   : para\n {#x}\n"), "<ol><li><dl><dt>t</dt><dd>para {<span class=\"tag\"><strong>#x</strong></span>}</dd></dl></li></ol>");
}
