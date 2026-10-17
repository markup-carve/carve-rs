fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_matched_comment_hides_its_payload_below_a_list_description() {
    assert_eq!(
        html("1. :: t\n   : para\n  %%%\n  c\n  %%%\n"),
        "<ol><li><dl><dt>t</dt><dd>para</dd></dl></li></ol>"
    );
}
