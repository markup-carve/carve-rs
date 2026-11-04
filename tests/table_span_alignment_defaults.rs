use carve::{lint_carve, to_carve, to_html};

#[test]
fn a_header_cell_holding_only_a_span_glyph_is_a_header_placeholder() {
    // carve#5091 (T4): `|= <|` widens `a`; the blocked `|= ^|` is an empty row header.
    let source = "|a|= <|\n|b|= ^|\n";
    let html = to_html(source);
    assert!(html.contains("<tr><td colspan=\"2\">a</td></tr>"), "{html}");
    assert!(
        html.contains("<tr><td>b</td><th scope=\"row\"></th></tr>"),
        "{html}"
    );
    assert_eq!(to_carve(source), "| a |= < |\n| b |= ^ |\n");
    let rules: Vec<_> = lint_carve(source).into_iter().map(|w| w.rule).collect();
    assert_eq!(rules, ["table-span-marker-blocked"]);

    // An escaped glyph is the literal header content.
    let literal = "|a|= \\<|\n|b|= \\^|\n";
    let html = to_html(literal);
    assert!(
        html.contains("<tr><td>a</td><th scope=\"row\">&lt;</th></tr>"),
        "{html}"
    );
    assert!(
        html.contains("<tr><td>b</td><th scope=\"row\">^</th></tr>"),
        "{html}"
    );
    assert_eq!(to_carve(literal), "| a |= \\< |\n| b |= \\^ |\n");
}

#[test]
fn a_spanning_header_sets_only_its_starting_column() {
    let native = to_html("|=> Wide|<|=~ Last|\n|a|b|c|\n");
    assert!(native.contains("<tr><td style=\"text-align: right;\">a</td><td>b</td><td style=\"text-align: center;\">c</td></tr>"), "{native}");

    let gfm = to_html("|> Wide|<|~ Last|\n|:--|:--|:--|\n|a|b|c|\n");
    assert!(gfm.contains("<tr><td style=\"text-align: right;\">a</td><td style=\"text-align: left;\">b</td><td style=\"text-align: center;\">c</td></tr>"), "{gfm}");
}

#[test]
fn explicit_header_row_promotion_does_not_widen_plain_markers() {
    let html = to_html("{header-rows=1}\n|> Wide|<|~ Last|\n|a|b|c|\n");
    assert!(
        html.contains("<th scope=\"col\" colspan=\"2\" style=\"text-align: right;\">Wide</th>"),
        "{html}"
    );
    assert!(
        html.contains("<tr><td>a</td><td>b</td><td>c</td></tr>"),
        "{html}"
    );
}

#[test]
fn vertical_defaults_stay_on_the_starting_column() {
    let html = to_html("|=>^ Wide|<|=~v Last|\n|a|b|c|\n");
    assert!(html.contains("<td style=\"text-align: right; vertical-align: top;\">a</td><td>b</td><td style=\"text-align: center; vertical-align: bottom;\">c</td>"), "{html}");
}

#[test]
fn a_vertical_marker_before_a_span_glyph_keeps_the_glyph_literal() {
    let html = to_html("|a|?^ <|\n");
    assert!(
        html.contains("<td>a</td><td style=\"vertical-align: top;\">&lt;</td>"),
        "{html}"
    );
}
