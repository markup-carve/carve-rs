use carve::to_html;

// carve#5012: the erased line leaves its quote prefix as a blank.
const ATTACHED: &str =
    "<blockquote>\n  <blockquote><p>a</p></blockquote>\n  <p>b</p>\n</blockquote>";

#[test]
fn an_indented_quoted_plus_skips_a_link_definition() {
    assert_eq!(to_html("> > a\n > +\n > [r]: /u\n > b\n"), ATTACHED);
}

#[test]
fn the_same_plus_skips_a_run_of_definitions() {
    assert_eq!(
        to_html("> > a\n > +\n > [r]: /u\n > [s]: /v\n > b\n"),
        ATTACHED
    );
}
