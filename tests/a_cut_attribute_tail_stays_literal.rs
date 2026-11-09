use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn an_attribute_block_cut_by_the_outer_bracket_run_stays_literal() {
    check(
        "[t[x](/u){title=\"]\"}y](/z)\n",
        "<p>[t<a href=\"/u\">x</a>{title=“]”}y](/z)</p>",
    );
    check(
        "a [t`c`{title=\"]\"}x](/u) b\n",
        "<p>a [t<code>c</code>{title=“]”}x](/u) b</p>",
    );
}

#[test]
fn an_editorial_comment_wins_over_a_failed_attribute_tail() {
    check(
        "[[][[]]{#1}][]#}\n",
        "<p>[[][[]]<span class=\"critic-comment\">1}][]</span></p>",
    );
}
