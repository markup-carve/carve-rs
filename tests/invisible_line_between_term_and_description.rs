//! An invisible line between a term and its `: ` line does not choose the next
//! line's owner (CARVE-P9-013), so a quote opened on the marker line keeps its
//! continuation at the body column.

const QUOTED_BODY: &str =
    "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>a\nb</p></blockquote>\n  </dd>\n</dl>";

#[test]
fn comment_between_term_and_description_keeps_the_quote_continuation() {
    assert_eq!(carve::to_html(":: t\n%%\n:  > a\n   > b\n"), QUOTED_BODY);
    assert_eq!(carve::to_html(":: t\n%% c\n:  > a\n   > b\n"), QUOTED_BODY);
}

#[test]
fn one_line_definitions_between_term_and_description_keep_the_quote_continuation() {
    assert_eq!(carve::to_html(":: t\n[r]: /u\n:  > a\n   > b\n"), QUOTED_BODY);
    assert_eq!(carve::to_html(":: t\n*[A]: x\n:  > a\n   > b\n"), QUOTED_BODY);
}

#[test]
fn quoted_description_keeps_the_continuation_after_a_comment() {
    assert_eq!(
        carve::to_html("> :: t\n> %%\n> :  > a\n>    > b\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <blockquote><p>a\nb</p></blockquote>\n    </dd>\n  </dl>\n</blockquote>"
    );
}
