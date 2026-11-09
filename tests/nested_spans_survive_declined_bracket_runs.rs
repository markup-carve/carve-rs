use carve::to_html;

#[test]
fn nested_spans_survive_declined_bracket_runs() {
    assert_eq!(
        to_html("[[a]{#1}][]#}\n").trim(),
        "<p>[<span id=\"1\">a</span>][]#}</p>"
    );
    assert_eq!(
        to_html("[[]{#1}[#}]{}\n").trim(),
        "<p><span><span id=\"1\"></span>[#}</span></p>"
    );
}
