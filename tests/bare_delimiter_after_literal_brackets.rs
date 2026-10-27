use carve::to_html;

#[test]
fn bare_delimiters_pair_after_literal_bracket_runs() {
    for (delimiter, tag) in [('*', "strong"), ('_', "u"), ('~', "s"), ('=', "mark")] {
        let source = format!("[{{{delimiter}a]{delimiter}}}\n[{{{delimiter}a]{delimiter}}}");
        let expected = format!("<p>[{{{delimiter}a]<{tag}>}}\n[{{{delimiter}a]</{tag}>}}</p>");
        assert_eq!(to_html(&source), expected, "{delimiter}");
    }
}
